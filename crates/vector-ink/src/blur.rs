//! Separable Gaussian blur for RGBA8 buffers (stamp post-process).
//!
//! Both passes run on premultiplied `f32`, so transparent pixels add no color
//! and no level is lost between passes. The result is dithered back to bytes:
//! a heavy blur is a slow gradient, and rounding it bands into rings.

/// In-place blur of straight-alpha RGBA8. `sigma` is in pixels.
pub fn gaussian_blur_rgba(rgba: &mut [u8], width: u32, height: u32, sigma: f32) {
    if sigma <= 0.05 || width == 0 || height == 0 || rgba.is_empty() {
        return;
    }
    let w = width as usize;
    let h = height as usize;
    let radius = (sigma * 3.0).ceil() as i32;
    let kernel = gaussian_kernel(sigma, radius);
    let mut tmp = vec![0.0_f32; w * h * 4];
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

/// Straight RGBA8 rows to horizontally blurred premultiplied `f32`.
fn blur_horizontal(src: &[u8], dst: &mut [f32], w: usize, h: usize, kernel: &[f32]) {
    let r = (kernel.len() / 2) as i32;
    let mut row = vec![0.0_f32; w * 4];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let a = src[i + 3] as f32;
            let k = a / 255.0;
            row[x * 4] = src[i] as f32 * k;
            row[x * 4 + 1] = src[i + 1] as f32 * k;
            row[x * 4 + 2] = src[i + 2] as f32 * k;
            row[x * 4 + 3] = a;
        }
        for x in 0..w {
            let mut acc = [0.0_f32; 4];
            for (ki, &kv) in kernel.iter().enumerate() {
                let sx = (x as i32 + ki as i32 - r).clamp(0, w as i32 - 1) as usize;
                for c in 0..4 {
                    acc[c] += row[sx * 4 + c] * kv;
                }
            }
            dst[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&acc);
        }
    }
}

/// Vertical pass over premultiplied `f32`, back to dithered straight RGBA8.
fn blur_vertical(src: &[f32], dst: &mut [u8], w: usize, h: usize, kernel: &[f32]) {
    let r = (kernel.len() / 2) as i32;
    let mut acc = vec![0.0_f32; w * 4];
    for y in 0..h {
        acc.fill(0.0);
        for (ki, &kv) in kernel.iter().enumerate() {
            let sy = (y as i32 + ki as i32 - r).clamp(0, h as i32 - 1) as usize;
            let line = &src[sy * w * 4..(sy + 1) * w * 4];
            for (a, s) in acc.iter_mut().zip(line) {
                *a += s * kv;
            }
        }
        for x in 0..w {
            let p = &acc[x * 4..x * 4 + 4];
            let o = (y * w + x) * 4;
            let (gx, gy) = (x as i64, y as i64);
            let alpha = p[3];
            if alpha <= 1.0e-4 {
                dst[o..o + 4].fill(0);
                continue;
            }
            let k = 255.0 / alpha;
            for c in 0..3 {
                dst[o + c] = crate::dither::quantize(p[c] * k, gx, gy);
            }
            dst[o + 3] = crate::dither::quantize(alpha, gx, gy);
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

    /// A hard disc of `rgba` at the center of a `side` square.
    fn disc(side: u32, radius: f32, rgba: [u8; 4]) -> Vec<u8> {
        let mut px = vec![0u8; (side * side * 4) as usize];
        let c = side as f32 * 0.5;
        for y in 0..side {
            for x in 0..side {
                let d = (x as f32 + 0.5 - c).hypot(y as f32 + 0.5 - c);
                if d < radius {
                    let i = ((y * side + x) * 4) as usize;
                    px[i..i + 4].copy_from_slice(&rgba);
                }
            }
        }
        px
    }

    #[test]
    fn a_heavy_blur_has_no_banded_rings() {
        let side = 256;
        let mut px = disc(side, 40.0, [200, 40, 40, 48]);
        gaussian_blur_rgba(&mut px, side, side, 24.0);
        let cy = side / 2;
        let alpha = |x: u32, y: u32| px[((y * side + x) * 4 + 3) as usize] as u32;
        // Band average over 8 rows through the center, walking right, over
        // the tail between 5% and 95% of the peak.
        let peak: u32 = (cy - 4..cy + 4).map(|y| alpha(cy, y)).sum();
        let (mut best, mut run, mut last) = (0usize, 0usize, None);
        for x in cy..side {
            let sum: u32 = (cy - 4..cy + 4).map(|y| alpha(x, y)).sum();
            if sum * 20 < peak || sum * 20 > peak * 19 {
                last = None;
                run = 0;
                continue;
            }
            run = if last == Some(sum) { run + 1 } else { 1 };
            last = Some(sum);
            best = best.max(run);
        }
        assert!(best <= 3, "blurred falloff holds one alpha for {best} px");
    }

    #[test]
    fn a_blurred_edge_keeps_its_color() {
        let side = 128;
        let mut px = disc(side, 20.0, [200, 40, 40, 255]);
        gaussian_blur_rgba(&mut px, side, side, 12.0);
        let y = side / 2;
        let faint = (side / 2..side)
            .map(|x| ((y * side + x) * 4) as usize)
            .find(|&i| px[i + 3] > 0 && px[i + 3] < 40)
            .expect("a faint edge pixel");
        let red = px[faint];
        assert!(
            red >= 185,
            "transparent pixels darkened the edge to red {red} at alpha {}",
            px[faint + 3]
        );
    }
}
