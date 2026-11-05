use super::{playback_window, sample_times, Job, VideoEvent, VideoFrame, PLAY_EDGE, STRIP_FRAMES};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use windows::Win32::Media::MediaFoundation::IMFSample;

mod aperture;
mod buffer;
mod clock;
mod diagnostics;
mod path;
use buffer::copy_sample;
use diagnostics::at;
mod reader;
mod strip;
pub(super) use reader::Open;
use reader::{configure_rgb32, seek};

pub fn run_strip(
    _job_rx: &crossbeam_channel::Receiver<Job>,
    ev_tx: &crossbeam_channel::Sender<VideoEvent>,
    play_token: &Arc<AtomicU64>,
    slot: &mut Option<Open>,
    job: &Job,
) -> bool {
    let Job::Strip {
        id,
        token,
        path,
        start,
        end,
    } = job
    else {
        return false;
    };
    let Some(open) = open_into(slot, path) else {
        let _ = ev_tx.send(VideoEvent::Failed {
            id: *id,
            token: *token,
        });
        return false;
    };
    let _ = ev_tx.send(VideoEvent::Probed {
        id: *id,
        token: *token,
        duration: open.duration,
    });
    let (win_start, win_end) = playback_window(*start, *end, open.duration);
    let times = sample_times(win_start, win_end, STRIP_FRAMES);
    let mut cursor = strip::StripCursor::new();
    for (index, time) in times.iter().copied().enumerate() {
        // A play request sets the token before its job is read. Stop this
        // filmstrip so playback does not wait behind the remaining seeks.
        if play_token.load(Ordering::SeqCst) != 0 {
            return true;
        }
        if let Some(frame) = cursor.sample(open, time) {
            let _ = ev_tx.send(VideoEvent::Strip {
                id: *id,
                token: *token,
                index: index as u16,
                time,
                frame,
            });
        } else {
            *slot = None;
            let _ = ev_tx.send(VideoEvent::Failed {
                id: *id,
                token: *token,
            });
            return false;
        }
    }
    cursor.receipt(times.len());
    let _ = ev_tx.send(VideoEvent::StripDone {
        id: *id,
        token: *token,
    });
    false
}

pub fn run_play(
    _job_rx: &crossbeam_channel::Receiver<Job>,
    ev_tx: &crossbeam_channel::Sender<VideoEvent>,
    play_token: &Arc<AtomicU64>,
    slot: &mut Option<Open>,
    job: Job,
) {
    let Job::Play {
        id,
        token,
        path,
        start,
        end,
        from,
        looped,
    } = job
    else {
        return;
    };
    let Some(open) = open_into(slot, &path) else {
        if play_token.load(Ordering::SeqCst) == token {
            play_token.store(0, Ordering::SeqCst);
        }
        let _ = ev_tx.send(VideoEvent::Failed { id, token });
        return;
    };
    let _ = ev_tx.send(VideoEvent::Probed {
        id,
        token,
        duration: open.duration,
    });
    let (win_start, win_end) = playback_window(start, end, open.duration);
    let mut origin_media = super::playback_origin(from, win_start, win_end);
    if seek(open, origin_media).is_err() {
        let _ = ev_tx.send(VideoEvent::Failed { id, token });
        return;
    }
    let mut clock = clock::FrameClock::new();
    let mut progress = Instant::now();
    let mut frames_in_pass = 0usize;
    loop {
        if progress.elapsed() > Duration::from_secs(8) {
            let _ = ev_tx.send(VideoEvent::Failed { id, token });
            return;
        }
        if play_token.load(Ordering::SeqCst) != token {
            return;
        }
        match next_sample(open, PLAY_EDGE) {
            Ok(Sample::Frame { time, frame }) => {
                diagnostics::first_decoded(time);
                if !time.is_finite() || time > clock.media_now(origin_media, Instant::now()) + 8.0 {
                    let _ = ev_tx.send(VideoEvent::Failed { id, token });
                    return;
                }
                progress = Instant::now();
                frames_in_pass += 1;
                if time > win_end + 0.05 {
                    if looped && win_end > win_start {
                        origin_media = win_start;
                        clock.reset();
                        if seek(open, win_start).is_err() {
                            let _ = ev_tx.send(VideoEvent::Failed { id, token });
                            return;
                        }
                        continue;
                    }
                    let _ = ev_tx.send(VideoEvent::Ended { id, token });
                    return;
                }
                // Seeking may return earlier keyframes. Do not display preroll
                // or let decoder startup advance the presentation clock.
                if time < origin_media {
                    continue;
                }
                let now = Instant::now();
                let due = clock.due(time, now);
                if due > now {
                    sleep_until(due, play_token, token);
                    if play_token.load(Ordering::SeqCst) != token {
                        return;
                    }
                } else if now.saturating_duration_since(due) > Duration::from_millis(120) {
                    continue;
                }
                diagnostics::first_frame(time);
                let _ = ev_tx.send(VideoEvent::Play {
                    id,
                    token,
                    time,
                    frame,
                });
            }
            Ok(Sample::Eos) => {
                if frames_in_pass == 0 {
                    let _ = ev_tx.send(VideoEvent::Failed { id, token });
                    return;
                }
                frames_in_pass = 0;
                if looped && win_end > win_start {
                    origin_media = win_start;
                    clock.reset();
                    if seek(open, win_start).is_err() {
                        let _ = ev_tx.send(VideoEvent::Failed { id, token });
                        return;
                    }
                    continue;
                }
                let _ = ev_tx.send(VideoEvent::Ended { id, token });
                return;
            }
            Ok(Sample::Gap) => {}
            Err(()) => {
                let _ = ev_tx.send(VideoEvent::Failed { id, token });
                return;
            }
        }
    }
}

fn sleep_until(due: Instant, play_token: &Arc<AtomicU64>, token: u64) {
    while Instant::now() < due {
        if play_token.load(Ordering::SeqCst) != token {
            return;
        }
        let left = due.saturating_duration_since(Instant::now());
        std::thread::sleep(left.min(Duration::from_millis(8)));
    }
}

fn open_into<'a>(slot: &'a mut Option<Open>, path: &'a Path) -> Option<&'a mut Open> {
    if slot.as_ref().is_some_and(|open| open.path == path) {
        return slot.as_mut();
    }
    *slot = Open::from_path(path).ok();
    slot.as_mut()
}

enum Sample {
    Frame { time: f32, frame: VideoFrame },
    Gap,
    Eos,
}

fn next_sample(open: &Open, max_edge: u32) -> Result<Sample, ()> {
    let mut flags = 0u32;
    let mut stamp = 0i64;
    let mut sample: Option<IMFSample> = None;
    unsafe {
        open.reader
            .ReadSample(
                open.stream,
                0,
                None,
                Some(&mut flags),
                Some(&mut stamp),
                Some(&mut sample),
            )
            .map_err(at("reader.read-sample"))?;
    }
    if flags & 1 != 0 {
        return Err(());
    }
    // Native changes require renegotiation; an output change already carries
    // the decoder's new current type and must not be reset to the old native size.
    if flags & 16 != 0 {
        configure_rgb32(&open.reader, open.stream)?;
    }
    if flags & 48 != 0 {
        diagnostics::format_change(flags, &open.reader, open.stream);
    }
    if flags & 2 != 0 && sample.is_none() {
        return Ok(Sample::Eos);
    }
    let Some(sample) = sample else {
        return Ok(if flags & 2 != 0 {
            Sample::Eos
        } else {
            Sample::Gap
        });
    };
    diagnostics::sample_times(stamp, &sample);
    // Preserve valid zero and signed preroll. A successfully decoded frame is
    // then mapped through the reader's once-established video presentation origin.
    let frame = copy_sample(&sample, &open.reader, open.stream, max_edge)?;
    let time = open.clip_time(stamp);
    Ok(Sample::Frame { time, frame })
}
