//! Separable Gaussian blur for RGBA8 buffers (stamp post-process).

/// In-place blur of RGBA8. `sigma` is in pixels.
pub fn gaussian_blur_rgba(rgba: &mut [u8], width: u32, height: u32, sigma: f32) {
    if sigma <= 0.05 || width == 0 || height == 0 || rgba.is_empty() {
        return;
    }
    let w = width as usize;
    let h = height as usize;
    let radius = (sigma * 3.0).ceil() as i32;
    let kernel = gaussian_kernel(sigma, radius);
    let mut tmp = rgba.to_vec();
    blur_horizontal(rgba, &mut tmp, w, h, &kernel);
    blur_vertical(&tmp, rgba, w, h, &kernel);
}

fn gaussian_kernel(sigma: f32, radius: i32) -> Vec<f32> {
    let mut k = Vec::new();
    let mut sum = 0.0_f32;
    for i in -radius..=radius {
        let x = i as f32;
        let v = (-0.5 * x * x / (sigma * sigma)).exp();
        k.push(v);
        sum += v;
    }
    for v in &mut k {
        *v /= sum;
    }
    k
}

fn blur_horizontal(src: &[u8], dst: &mut [u8], w: usize, h: usize, kernel: &[f32]) {
    let r = (kernel.len() / 2) as i32;
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0.0_f32; 4];
            for (ki, &kv) in kernel.iter().enumerate() {
                let dx = ki as i32 - r;
                let sx = (x as i32 + dx).clamp(0, w as i32 - 1) as usize;
                let i = (y * w + sx) * 4;
                for c in 0..4 {
                    acc[c] += src[i + c] as f32 * kv;
                }
            }
            let o = (y * w + x) * 4;
            for c in 0..4 {
                dst[o + c] = acc[c].round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

fn blur_vertical(src: &[u8], dst: &mut [u8], w: usize, h: usize, kernel: &[f32]) {
    let r = (kernel.len() / 2) as i32;
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0.0_f32; 4];
            for (ki, &kv) in kernel.iter().enumerate() {
                let dy = ki as i32 - r;
                let sy = (y as i32 + dy).clamp(0, h as i32 - 1) as usize;
                let i = (sy * w + x) * 4;
                for c in 0..4 {
                    acc[c] += src[i + c] as f32 * kv;
                }
            }
            let o = (y * w + x) * 4;
            for c in 0..4 {
                dst[o + c] = acc[c].round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_sigma_is_noop() {
        let mut px = vec![255u8, 0, 0, 255, 0, 0, 0, 0];
        gaussian_blur_rgba(&mut px, 2, 1, 0.0);
        assert_eq!(px[0], 255);
    }
}
