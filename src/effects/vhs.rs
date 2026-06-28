use super::{Effect, EffectParams};
use image::{DynamicImage, GenericImageView, ImageBuffer};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VhsParams {
    pub scanline_intensity: f32,
    pub scanline_gap: u32,
    pub color_bleed: f32,
    pub noise: f32,
    pub noise_seed: u64,
    pub vignette: f32,
    pub barrel: f32,
    pub tracking_distortion: f32,
}

impl Default for VhsParams {
    fn default() -> Self {
        Self {
            scanline_intensity: 0.25,
            scanline_gap: 2,
            color_bleed: 4.0,
            noise: 0.04,
            noise_seed: 0,
            vignette: 0.4,
            barrel: 0.05,
            tracking_distortion: 0.0,
        }
    }
}

fn lcg(s: u64) -> u64 {
    s.wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407)
}

pub struct Vhs(pub VhsParams);

impl Effect for Vhs {
    fn name(&self) -> &str {
        "VHS / CRT"
    }

    fn apply(&self, img: &DynamicImage) -> DynamicImage {
        let p = &self.0;
        let (w, h) = img.dimensions();
        let rgba = img.to_rgba8();

        // --- Barrel distortion pass ---
        let barrel_raw: Vec<u8> = if p.barrel > 0.001 {
            let cx = w as f32 / 2.0;
            let cy = h as f32 / 2.0;
            let k = p.barrel;
            let src = rgba.as_raw();
            let stride = w as usize * 4;

            (0..h)
                .into_par_iter()
                .flat_map(|y| {
                    (0..w)
                        .map(|x| {
                            let ox = x as f32;
                            let oy = y as f32;
                            let nx = (ox - cx) / cx;
                            let ny = (oy - cy) / cy;
                            let r2 = nx * nx + ny * ny;
                            let factor = 1.0 + k * r2;
                            let sx = (nx / factor * cx + cx).clamp(0.0, w as f32 - 1.0);
                            let sy = (ny / factor * cy + cy).clamp(0.0, h as f32 - 1.0);
                            let x0 = sx.floor() as u32;
                            let y0 = sy.floor() as u32;
                            let x1 = (x0 + 1).min(w - 1);
                            let y1 = (y0 + 1).min(h - 1);
                            let fx = sx - sx.floor();
                            let fy = sy - sy.floor();

                            let idx = |px: u32, py: u32| -> usize {
                                (py as usize * stride) + (px as usize * 4)
                            };
                            let i00 = idx(x0, y0);
                            let i10 = idx(x1, y0);
                            let i01 = idx(x0, y1);
                            let i11 = idx(x1, y1);

                            let a00 = 1.0 - fx;
                            let a10 = fx;
                            let b0 = 1.0 - fy;
                            let b1 = fy;

                            [
                                (src[i00] as f32 * a00 * b0
                                    + src[i10] as f32 * a10 * b0
                                    + src[i01] as f32 * a00 * b1
                                    + src[i11] as f32 * a10 * b1)
                                    as u8,
                                (src[i00 + 1] as f32 * a00 * b0
                                    + src[i10 + 1] as f32 * a10 * b0
                                    + src[i01 + 1] as f32 * a00 * b1
                                    + src[i11 + 1] as f32 * a10 * b1)
                                    as u8,
                                (src[i00 + 2] as f32 * a00 * b0
                                    + src[i10 + 2] as f32 * a10 * b0
                                    + src[i01 + 2] as f32 * a00 * b1
                                    + src[i11 + 2] as f32 * a10 * b1)
                                    as u8,
                                (src[i00 + 3] as f32 * a00 * b0
                                    + src[i10 + 3] as f32 * a10 * b0
                                    + src[i01 + 3] as f32 * a00 * b1
                                    + src[i11 + 3] as f32 * a10 * b1)
                                    as u8,
                            ]
                        })
                        .flat_map(|px| px)
                        .collect::<Vec<_>>()
                })
                .collect()
        } else {
            rgba.as_raw().clone()
        };
        let barrel_pass = ImageBuffer::<image::Rgba<u8>, _>::from_raw(w, h, barrel_raw).unwrap();

        // --- Color bleed: horizontal chroma smear ---
        // Smear R and B channels (which carry chroma info), leave G (luma proxy) sharper
        let bleed_pass = if p.color_bleed > 0.01 {
            let bleed = p.color_bleed.max(1.0) as usize;
            let src = barrel_pass.as_raw();
            let stride = w as usize * 4;

            let norm: f32 = (0..=bleed).map(|i| 1.0 / (i as f32 + 1.0)).sum();

            let raw: Vec<u8> = (0..h)
                .into_par_iter()
                .flat_map(|y| {
                    let tracking_offset = if p.tracking_distortion > 0.001 {
                        let rng = lcg(lcg(p.noise_seed ^ y as u64) ^ 0xdeadbeef);
                        let noise = (rng >> 33) as f32 / (u32::MAX as f32) - 0.5;
                        if noise.abs() > (1.0 - p.tracking_distortion) {
                            (noise * p.tracking_distortion * 30.0) as i32
                        } else {
                            0
                        }
                    } else {
                        0
                    };

                    (0..w)
                        .flat_map(|x| {
                            let sx = (x as i32 + tracking_offset).clamp(0, w as i32 - 1) as u32;
                            let center_idx = (y as usize * stride) + (sx as usize * 4);
                            let center_g = src[center_idx + 1];
                            let center_a = src[center_idx + 3];

                            let mut r_acc = 0f32;
                            let mut b_acc = 0f32;
                            for off in 0..=bleed {
                                let bx = sx.saturating_sub(off as u32) as usize;
                                let bidx = (y as usize * stride) + (bx * 4);
                                let weight = 1.0 / (off as f32 + 1.0);
                                r_acc += src[bidx] as f32 * weight;
                                b_acc += src[bidx + 2] as f32 * weight;
                            }
                            let r = (r_acc / norm) as u8;
                            let b = (b_acc / norm) as u8;
                            [r, center_g, b, center_a]
                        })
                        .collect::<Vec<_>>()
                })
                .collect();

            ImageBuffer::<image::Rgba<u8>, _>::from_raw(w, h, raw).unwrap()
        } else {
            barrel_pass
        };

        // --- Scanlines + noise + vignette (parallel by row) ---
        let gap = p.scanline_gap.max(1);
        let cx = w as f32 / 2.0;
        let cy = h as f32 / 2.0;
        let max_dist = cx.hypot(cy);
        let src = bleed_pass.as_raw();
        let stride = w as usize * 4;

        let out: Vec<u8> = (0..h)
            .into_par_iter()
            .flat_map(|y| {
                let scanline_darken = if y % gap == 0 {
                    1.0 - p.scanline_intensity
                } else {
                    1.0
                };
                let vy = if p.vignette > 0.001 {
                    let dy = y as f32 - cy;
                    Some(dy)
                } else {
                    None
                };

                (0..w)
                    .map(|x| {
                        let idx = (y as usize * stride) + (x as usize * 4);
                        let mut r = src[idx] as f32;
                        let mut g = src[idx + 1] as f32;
                        let mut b = src[idx + 2] as f32;
                        let a = src[idx + 3];

                        // Scanline darkening
                        if scanline_darken < 1.0 {
                            r *= scanline_darken;
                            g *= scanline_darken;
                            b *= scanline_darken;
                        }

                        // Noise
                        if p.noise > 0.001 {
                            let rng =
                                lcg(lcg(p.noise_seed ^ (y as u64 * w as u64 + x as u64))
                                    ^ 0xfeedbabe);
                            let n =
                                ((rng >> 33) as f32 / (u32::MAX as f32) - 0.5) * p.noise * 255.0;
                            r = (r + n).clamp(0.0, 255.0);
                            g = (g + n).clamp(0.0, 255.0);
                            b = (b + n).clamp(0.0, 255.0);
                        }

                        // Vignette
                        if let Some(dy) = vy {
                            let dx = x as f32 - cx;
                            let dist = dx.hypot(dy) / max_dist;
                            let v = 1.0 - (dist * dist) * p.vignette;
                            r *= v;
                            g *= v;
                            b *= v;
                        }

                        [
                            r.clamp(0.0, 255.0) as u8,
                            g.clamp(0.0, 255.0) as u8,
                            b.clamp(0.0, 255.0) as u8,
                            a,
                        ]
                    })
                    .flat_map(|px| px)
                    .collect::<Vec<_>>()
            })
            .collect();

        let out = ImageBuffer::<image::Rgba<u8>, _>::from_raw(w, h, out).unwrap();
        DynamicImage::ImageRgba8(out)
    }

    fn params(&self) -> EffectParams {
        EffectParams::Vhs(self.0.clone())
    }
    fn set_params(&mut self, p: EffectParams) {
        if let EffectParams::Vhs(v) = p {
            self.0 = v;
        }
    }
    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(Vhs(self.0.clone()))
    }
}
