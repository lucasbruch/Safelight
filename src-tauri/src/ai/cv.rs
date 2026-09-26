//! Classic image measurements that need no ML model: sharpness, motion blur,
//! exposure clipping, horizon tilt and a perceptual hash for near-duplicates.

use image::{GrayImage, RgbImage};

/// Work size for the measurements: large enough to see focus, small enough to be fast.
pub const WORK_MAX: u32 = 1024;

pub struct Measurements {
    /// Sharpness of the sharpest region (0–100, log scale).
    pub sharpness: f64,
    /// Sharpness inside a region of interest (a face), if given.
    pub roi_sharpness: Option<f64>,
    /// How directional the edges are (0–1); high + soft = motion blur.
    pub coherence: f64,
    pub over_exposed: f64,
    pub under_exposed: f64,
    pub horizon_tilt: Option<f64>,
    pub phash: u64,
}

pub fn gray(rgb: &RgbImage) -> GrayImage {
    image::imageops::grayscale(rgb)
}

/// Sobel gradients → (gx, gy) per pixel (borders zero).
fn sobel(g: &GrayImage) -> (Vec<f32>, Vec<f32>) {
    let (w, h) = (g.width() as usize, g.height() as usize);
    let px = g.as_raw();
    let mut gx = vec![0f32; w * h];
    let mut gy = vec![0f32; w * h];
    for y in 1..h.saturating_sub(1) {
        for x in 1..w - 1 {
            let p = |dx: isize, dy: isize| px[((y as isize + dy) as usize) * w + (x as isize + dx) as usize] as f32;
            gx[y * w + x] = (p(1, -1) + 2.0 * p(1, 0) + p(1, 1)) - (p(-1, -1) + 2.0 * p(-1, 0) + p(-1, 1));
            gy[y * w + x] = (p(-1, 1) + 2.0 * p(0, 1) + p(1, 1)) - (p(-1, -1) + 2.0 * p(0, -1) + p(1, -1));
        }
    }
    (gx, gy)
}

/// Variance of the Laplacian over a rectangle.
fn laplacian_var(g: &GrayImage, x0: u32, y0: u32, x1: u32, y1: u32) -> f64 {
    let w = g.width() as usize;
    let px = g.as_raw();
    let (mut sum, mut sq, mut n) = (0f64, 0f64, 0f64);
    for y in (y0.max(1) as usize)..(y1.min(g.height() - 1) as usize) {
        for x in (x0.max(1) as usize)..(x1.min(g.width() - 1) as usize) {
            let c = px[y * w + x] as f64;
            let l = px[y * w + x - 1] as f64 + px[y * w + x + 1] as f64 + px[(y - 1) * w + x] as f64
                + px[(y + 1) * w + x] as f64
                - 4.0 * c;
            sum += l;
            sq += l * l;
            n += 1.0;
        }
    }
    if n < 16.0 {
        return 0.0;
    }
    let mean = sum / n;
    (sq / n - mean * mean).max(0.0)
}

/// Maps Laplacian variance (roughly 1…5000+) onto 0–100.
fn score(var: f64) -> f64 {
    ((var + 1.0).log10() / 3.7 * 100.0).clamp(0.0, 100.0)
}

/// Normalised region in 0..1 coordinates.
#[derive(Clone, Copy, Debug)]
pub struct Roi {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

pub fn measure(rgb: &RgbImage, roi: Option<Roi>) -> Measurements {
    let g = gray(rgb);
    let (w, h) = g.dimensions();

    // Sharpness: split into a 6×6 grid, score each tile and take the 2nd best
    // (robust against one noisy tile). The subject only needs to be sharp somewhere.
    let (gw, gh) = (6u32, 6u32);
    let mut tiles: Vec<f64> = vec![];
    for ty in 0..gh {
        for tx in 0..gw {
            let (x0, y0) = (tx * w / gw, ty * h / gh);
            let (x1, y1) = ((tx + 1) * w / gw, (ty + 1) * h / gh);
            tiles.push(laplacian_var(&g, x0, y0, x1, y1));
        }
    }
    tiles.sort_by(|a, b| b.total_cmp(a));
    let sharpness = score(tiles.get(1).copied().unwrap_or(0.0));

    let roi_sharpness = roi.map(|r| {
        let x0 = (r.x * w as f32).max(0.0) as u32;
        let y0 = (r.y * h as f32).max(0.0) as u32;
        let x1 = ((r.x + r.w) * w as f32).min(w as f32) as u32;
        let y1 = ((r.y + r.h) * h as f32).min(h as f32) as u32;
        score(laplacian_var(&g, x0, y0, x1, y1))
    });

    // Structure tensor over the whole frame → edge directionality.
    let (gx, gy) = sobel(&g);
    let (mut sxx, mut syy, mut sxy) = (0f64, 0f64, 0f64);
    for i in 0..gx.len() {
        let (a, b) = (gx[i] as f64, gy[i] as f64);
        sxx += a * a;
        syy += b * b;
        sxy += a * b;
    }
    let tr = sxx + syy;
    let det_term = ((sxx - syy).powi(2) + 4.0 * sxy * sxy).sqrt();
    let coherence = if tr > 0.0 { det_term / tr } else { 0.0 };

    // Exposure: share of pixels clipped in any channel / crushed in all.
    let (mut over, mut under) = (0usize, 0usize);
    for p in rgb.pixels() {
        let [r, gg, b] = p.0;
        if r >= 250 || gg >= 250 || b >= 250 {
            over += 1;
        }
        if r <= 4 && gg <= 4 && b <= 4 {
            under += 1;
        }
    }
    let n = (rgb.width() * rgb.height()).max(1) as f64;

    Measurements {
        sharpness,
        roi_sharpness,
        coherence,
        over_exposed: over as f64 / n,
        under_exposed: under as f64 / n,
        horizon_tilt: horizon(&g, &gx, &gy),
        phash: dhash(&g),
    }
}

/// Finds a dominant long straight edge within ±10° of horizontal or vertical
/// (a horizon, a building edge) and returns its tilt in degrees.
fn horizon(g: &GrayImage, gx: &[f32], gy: &[f32]) -> Option<f64> {
    let (w, h) = (g.width() as usize, g.height() as usize);
    let mags: Vec<f32> = gx.iter().zip(gy).map(|(a, b)| (a * a + b * b).sqrt()).collect();
    let mut sorted: Vec<f32> = mags.iter().copied().filter(|m| *m > 0.0).collect();
    if sorted.len() < 1000 {
        return None;
    }
    let k = sorted.len() * 92 / 100;
    let thresh = *sorted.select_nth_unstable_by(k, |a, b| a.total_cmp(b)).1;
    let thresh = thresh.max(60.0);

    // Edge pixels whose gradient says "part of a near-horizontal line" or
    // "part of a near-vertical line", collected once.
    let (mut hpts, mut vpts) = (vec![], vec![]);
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = y * w + x;
            if mags[i] < thresh {
                continue;
            }
            let (ax, ay) = (gx[i].abs(), gy[i].abs());
            if ay >= ax * 2.0 {
                hpts.push((x as f64, y as f64));
            } else if ax >= ay * 2.0 {
                vpts.push((x as f64, y as f64));
            }
        }
    }

    // Hough vote over -10°..10° in 0.25° steps for both line families.
    // For either family the image tilt comes out as -angle.
    let diag = ((w * w + h * h) as f64).sqrt() as usize + 1;
    let mut best: (usize, f64) = (0, 0.0);
    let mut acc = vec![0usize; diag * 2];
    for (pts, vertical) in [(&hpts, false), (&vpts, true)] {
        for i in -40..=40 {
            let deg = i as f64 * 0.25;
            let t = deg.to_radians();
            let (c, s) = (t.cos(), t.sin());
            acc.iter_mut().for_each(|a| *a = 0);
            for &(x, y) in pts.iter() {
                let rho = if vertical { x * c - y * s } else { y * c + x * s };
                acc[(rho as isize + diag as isize) as usize] += 1;
            }
            let peak = *acc.iter().max().unwrap_or(&0);
            if peak > best.0 {
                best = (peak, -deg);
            }
        }
    }
    // A line must span at least ~45% of the frame to count.
    let min_len = (w.min(h) as f64 * 0.45) as usize;
    if best.0 >= min_len {
        Some(best.1)
    } else {
        None
    }
}

/// 64-bit difference hash of a 9×8 thumbnail.
pub fn dhash(g: &GrayImage) -> u64 {
    let small = image::imageops::resize(g, 9, 8, image::imageops::FilterType::Triangle);
    let mut bits = 0u64;
    for y in 0..8 {
        for x in 0..8 {
            bits <<= 1;
            if small.get_pixel(x, y).0[0] > small.get_pixel(x + 1, y).0[0] {
                bits |= 1;
            }
        }
    }
    bits
}

pub fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    fn checker(size: u32, cell: u32) -> RgbImage {
        RgbImage::from_fn(size, size, |x, y| if ((x / cell) + (y / cell)) % 2 == 0 { Rgb([240, 240, 240]) } else { Rgb([20, 20, 20]) })
    }

    #[test]
    fn blur_lowers_sharpness() {
        let sharp = checker(512, 8);
        let blurred = image::imageops::blur(&sharp, 6.0);
        let a = measure(&sharp, None).sharpness;
        let b = measure(&blurred, None).sharpness;
        assert!(a > b + 20.0, "sharp {a} vs blurred {b}");
    }

    #[test]
    fn exposure_detects_clipping() {
        let white = RgbImage::from_pixel(64, 64, Rgb([255, 255, 255]));
        let m = measure(&white, None);
        assert!(m.over_exposed > 0.99);
        assert!(m.under_exposed < 0.01);
    }

    #[test]
    fn tilted_horizon_is_found() {
        // Sky above a line tilted by 3°.
        let (w, h) = (600u32, 400u32);
        let t = 3f64.to_radians().tan();
        let img = RgbImage::from_fn(w, h, |x, y| {
            let line = 200.0 + (x as f64 - 300.0) * t;
            if (y as f64) < line { Rgb([180, 200, 230]) } else { Rgb([40, 60, 30]) }
        });
        let tilt = measure(&img, None).horizon_tilt.expect("horizon");
        assert!((tilt.abs() - 3.0).abs() < 0.6, "tilt {tilt}");
    }

    #[test]
    fn dhash_similar_images_close() {
        let a = checker(256, 32);
        let b = image::imageops::blur(&a, 1.0);
        let c = checker(256, 7);
        let (ha, hb, hc) = (dhash(&gray(&a)), dhash(&gray(&b)), dhash(&gray(&c)));
        assert!(hamming(ha, hb) < hamming(ha, hc) || hamming(ha, hb) <= 6);
    }
}
