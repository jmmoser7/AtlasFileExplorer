//! Nonblocking D3D staging. Capturing new content never depends on whether
//! an older copy has finished, and retirement prefers the newest ready copy.

use super::*;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CPU_ACCESS_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_FLAG_DO_NOT_WAIT, D3D11_MAP_READ,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess;

#[derive(Default)]
pub(super) struct PendingCopies {
    slots: [Option<u64>; 2],
    sequence: u64,
}

impl PendingCopies {
    pub(super) fn has_pending(&self) -> bool {
        self.slots.iter().any(Option::is_some)
    }

    fn free_slot(&self) -> Option<usize> {
        self.slots.iter().position(Option::is_none)
    }

    fn newest_first(&self) -> [usize; 2] {
        if self.slots[0] < self.slots[1] {
            [1, 0]
        } else {
            [0, 1]
        }
    }

    fn issued(&mut self, slot: usize) {
        debug_assert!(self.slots[slot].is_none());
        self.sequence += 1;
        self.slots[slot] = Some(self.sequence);
    }

    fn retired(&mut self, slot: usize) {
        let Some(sequence) = self.slots[slot] else {
            return;
        };
        for pending in &mut self.slots {
            if pending.is_some_and(|older| older <= sequence) {
                *pending = None;
            }
        }
    }
}

impl Webview2Host {
    pub(super) fn read_frame(&mut self, id: NodeId) -> Option<super::super::board_web::WebFrame> {
        let frame = self.retire_frame(id);
        self.issue_frame(id);
        frame
    }

    pub(super) fn retire_frame(&mut self, id: NodeId) -> Option<super::super::board_web::WebFrame> {
        let _span = atlas_core::session_log::span("slate.web.readback");
        let view = self.views.get_mut(&id)?;
        for slot in view.pending_copy.newest_first() {
            if view.pending_copy.slots[slot].is_none() {
                continue;
            }
            let Some(staging) = view.staging[slot].as_ref() else {
                continue;
            };
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            // Every map is a probe. A busy newest copy can fall back to an
            // older ready copy, and never prevents acquisition in issue_frame.
            if unsafe {
                self.context.Map(
                    staging,
                    0,
                    D3D11_MAP_READ,
                    D3D11_MAP_FLAG_DO_NOT_WAIT.0 as u32,
                    Some(&mut mapped),
                )
            }
            .is_err()
            {
                continue;
            }
            let image = std::sync::Arc::new(bgra_to_color_image(
                &mapped,
                view.size.0 as usize,
                view.size.1 as usize,
            ));
            unsafe { self.context.Unmap(staging, 0) };
            view.pending_copy.retired(slot);
            if super::super::board_web::web_frame_has_content(&image) {
                view.last = Some(std::sync::Arc::clone(&image));
                return Some(image);
            }
        }
        None
    }

    pub(super) fn issue_frame(&mut self, id: NodeId) {
        let Some(view) = self.views.get_mut(&id) else {
            return;
        };
        let Some(pool) = view.pool.as_ref() else {
            return;
        };
        let Some(write) = view.pending_copy.free_slot() else {
            return;
        };
        // WGC keeps two frames. Close the older one and copy only the newest;
        // stale frames from a previous resolution never enter staging.
        let mut newest = pool.TryGetNextFrame().ok();
        if let Ok(frame) = pool.TryGetNextFrame() {
            if let Some(old) = newest.replace(frame) {
                let _ = old.Close();
            }
        }
        let Some(frame) = newest else { return };
        let source = frame
            .Surface()
            .ok()
            .and_then(|surface| surface.cast::<IDirect3DDxgiInterfaceAccess>().ok())
            .and_then(|access| unsafe { access.GetInterface::<ID3D11Texture2D>() }.ok());
        if let Some(source) = source {
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            unsafe { source.GetDesc(&mut desc) };
            if (desc.Width, desc.Height) == view.size {
                if view.staging[write].is_none() {
                    view.staging[write] =
                        create_staging(&self.device, view.size.0, view.size.1).ok();
                }
                if let Some(staging) = &view.staging[write] {
                    unsafe {
                        self.context.CopyResource(staging, &source);
                        self.context.Flush();
                    }
                    view.pending_copy.issued(write);
                }
            }
        }
        let _ = frame.Close();
    }
}

fn create_staging(device: &ID3D11Device, w: u32, h: u32) -> windows::core::Result<ID3D11Texture2D> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: w,
        Height: h,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0,
    };
    let mut tex = None;
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut tex))? };
    Ok(tex.unwrap())
}

/// The captured surface is BGRA with a row pitch that is not the row width.
fn bgra_to_color_image(mapped: &D3D11_MAPPED_SUBRESOURCE, w: usize, h: usize) -> egui::ColorImage {
    let mut pixels = Vec::with_capacity(w * h);
    let pitch = mapped.RowPitch as usize;
    let base = mapped.pData as *const u8;
    for y in 0..h {
        let row = unsafe { std::slice::from_raw_parts(base.add(y * pitch), w * 4) };
        pixels.extend(
            row.as_chunks::<4>()
                .0
                .iter()
                .map(|p| egui::Color32::from_rgba_premultiplied(p[2], p[1], p[0], p[3])),
        );
    }
    egui::ColorImage {
        size: [w, h],
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgra_conversion_preserves_premultiplied_colors_and_skips_row_padding() {
        let mut source = vec![
            30, 20, 10, 128, 60, 50, 40, 255, 99, 99, 99, 99, 3, 2, 1, 4, 0, 0, 0, 0, 88, 88, 88,
            88,
        ];
        let mapped = D3D11_MAPPED_SUBRESOURCE {
            pData: source.as_mut_ptr().cast(),
            RowPitch: 12,
            DepthPitch: 24,
        };
        let image = bgra_to_color_image(&mapped, 2, 2);
        assert_eq!(image.size, [2, 2]);
        assert_eq!(
            image
                .pixels
                .iter()
                .map(|p| p.to_array())
                .collect::<Vec<_>>(),
            [
                [10, 20, 30, 128],
                [40, 50, 60, 255],
                [1, 2, 3, 4],
                [0, 0, 0, 0]
            ]
        );
    }

    /// CPU conversion only: excludes GPU map, WebView2, upload and display.
    #[test]
    #[ignore = "bounded generated-only CPU readback conversion measurements"]
    fn web_readback_conversion_cost() {
        use std::hint::black_box;
        use std::time::Instant;

        for (width, height) in [(1280usize, 720usize), (1920, 1080), (3840, 2160)] {
            let pitch = width * 4 + 64;
            let mut source = vec![0u8; pitch * height];
            for y in 0..height {
                for (x, pixel) in source[y * pitch..y * pitch + width * 4]
                    .chunks_exact_mut(4)
                    .enumerate()
                {
                    pixel.copy_from_slice(&[(x % 256) as u8, (y % 256) as u8, 127, 255]);
                }
            }
            let mapped = D3D11_MAPPED_SUBRESOURCE {
                pData: source.as_mut_ptr().cast(),
                RowPitch: pitch as u32,
                DepthPitch: (pitch * height) as u32,
            };
            for _ in 0..5 {
                black_box(bgra_to_color_image(&mapped, width, height));
            }
            let mut samples = Vec::with_capacity(40);
            for _ in 0..40 {
                let started = Instant::now();
                let image = black_box(bgra_to_color_image(&mapped, width, height));
                samples.push(started.elapsed().as_secs_f64() * 1000.0);
                assert_eq!(image.pixels.len(), width * height);
                assert_eq!(image.pixels[0].to_array(), [127, 0, 0, 255]);
                drop(image);
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "web_readback_conversion {}",
                serde_json::json!({
                    "width": width, "height": height, "row_pitch": pitch,
                    "output_bytes": width * height * 4, "samples_ms": samples,
                    "p50_ms": samples[19], "p95_ms": samples[37],
                    "p99_ms": samples[39], "max_ms": samples[39],
                    "scope": "CPU allocation and BGRA conversion only; no GPU map/upload/display"
                })
            );
        }
    }

    #[test]
    fn busy_pending_copy_keeps_a_second_slot_available() {
        let mut copies = PendingCopies::default();
        copies.issued(0);
        assert!(copies.has_pending());
        assert_eq!(copies.free_slot(), Some(1));
        copies.issued(1);
        assert_eq!(
            copies.free_slot(),
            None,
            "a third capture cannot overwrite an in-flight copy"
        );
        assert_eq!(copies.newest_first(), [1, 0]);
    }

    #[test]
    fn retiring_newest_copy_discards_the_older_pending_result() {
        let mut copies = PendingCopies::default();
        copies.issued(0);
        copies.issued(1);
        copies.retired(1);
        assert!(
            !copies.has_pending(),
            "an older completion cannot replace newer pixels"
        );
        assert_eq!(copies.free_slot(), Some(0));
    }

    #[test]
    fn older_ready_copy_preserves_the_newer_busy_copy() {
        let mut copies = PendingCopies::default();
        copies.issued(0);
        copies.issued(1);
        copies.retired(0);
        assert!(copies.has_pending());
        assert_eq!(copies.free_slot(), Some(0));
        copies.issued(0);
        assert_eq!(copies.newest_first(), [0, 1]);
        copies.retired(1);
        assert!(
            copies.slots[0].is_some(),
            "retiring an older slot must preserve the next capture"
        );
    }
}
