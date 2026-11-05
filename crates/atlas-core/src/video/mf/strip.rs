use super::{next_sample, seek, Open, Sample};
use crate::video::{
    sampling::{self, SamplePlan},
    VideoFrame, STRIP_EDGE,
};

enum Scan {
    Reached,
    Eos,
    Exhausted,
}
/// One bounded scaled frame follows the decoder through monotonic strip targets.
/// Reusing the reader avoids re-decoding the same GOP prefix96times.
pub(super) struct StripCursor {
    last: Option<(f32, VideoFrame)>,
    eos: bool,
    seeks: usize,
    reads: usize,
    frames: usize,
}
impl StripCursor {
    pub fn new() -> Self {
        Self {
            last: None,
            eos: false,
            seeks: 0,
            reads: 0,
            frames: 0,
        }
    }
    pub fn sample(&mut self, open: &Open, target: f32) -> Option<VideoFrame> {
        let last_time = self.last.as_ref().map(|(time, _)| *time);
        let plan = sampling::plan(last_time, target, open.frame_rate());
        if plan == SamplePlan::Cached || (self.eos && last_time.is_some_and(|time| target >= time))
        {
            return self.duplicate();
        }
        if plan == SamplePlan::Forward {
            match self.scan(open, target, sampling::FORWARD_LIMIT)? {
                Scan::Reached | Scan::Eos => return self.duplicate(),
                Scan::Exhausted => {} // Variable-rate/bad estimate: use the bounded seek path.
            }
        }
        seek(open, target).ok()?;
        self.seeks += 1;
        self.last = None;
        self.eos = false;
        self.scan(open, target, 120)?;
        self.duplicate()
    }
    fn scan(&mut self, open: &Open, target: f32, limit: usize) -> Option<Scan> {
        for _ in 0..limit {
            self.reads += 1;
            match next_sample(open, STRIP_EDGE).ok()? {
                Sample::Frame { time, frame } => {
                    self.frames += 1;
                    self.last = Some((time, frame));
                    if time + 0.03 >= target {
                        return Some(Scan::Reached);
                    }
                }
                Sample::Gap => {}
                Sample::Eos => {
                    self.eos = true;
                    return Some(Scan::Eos);
                }
            }
        }
        Some(Scan::Exhausted)
    }
    fn duplicate(&self) -> Option<VideoFrame> {
        let (_, frame) = self.last.as_ref()?;
        let mut rgba = Vec::new();
        rgba.try_reserve_exact(frame.rgba.len()).ok()?;
        rgba.extend_from_slice(&frame.rgba);
        Some(VideoFrame {
            w: frame.w,
            h: frame.h,
            rgba,
        })
    }
    pub fn receipt(&self, targets: usize) {
        super::diagnostics::strip_samples(targets, self.seeks, self.reads, self.frames);
    }
}
