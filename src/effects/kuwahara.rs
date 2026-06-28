use super::{Effect, EffectParams};
use image::{DynamicImage, ImageBuffer};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KuwaharaParams {
    pub radius: i32,
}

impl Default for KuwaharaParams {
    fn default() -> Self {
        Self { radius: 6 }
    }
}

pub struct Kuwahara(pub KuwaharaParams);

impl Effect for Kuwahara {
    fn name(&self) -> &str {
        "Kuwahara"
    }

    fn apply(&self, img: &DynamicImage) -> DynamicImage {
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        let r = self.0.radius.max(1);

        // Parallelize by row, each row produces Vec<u8> (raw RGBA)
        let raw: Vec<u8> = (0..h)
            .into_par_iter()
            .flat_map(|y| {
                (0..w)
                    .map(|x| {
                        let orig = rgba.get_pixel(x, y);
                        let quadrants = [(-r, -r), (0i32, -r), (-r, 0i32), (0i32, 0i32)];
                        let mut best_var = f32::MAX;
                        let mut best_mean = [0f32; 3];

                        for (qx, qy) in quadrants {
                            let mut sum = [0f32; 3];
                            let mut sum_sq = [0f32; 3];
                            let mut count = 0f32;
                            for dy in 0..=r {
                                for dx in 0..=r {
                                    let sx = (x as i32 + qx + dx).clamp(0, w as i32 - 1) as u32;
                                    let sy = (y as i32 + qy + dy).clamp(0, h as i32 - 1) as u32;
                                    let p = rgba.get_pixel(sx, sy);
                                    for c in 0..3 {
                                        let v = p[c] as f32;
                                        sum[c] += v;
                                        sum_sq[c] += v * v;
                                    }
                                    count += 1.0;
                                }
                            }
                            let mean = sum.map(|s| s / count);
                            let mean_lum = 0.2126 * mean[0] + 0.7152 * mean[1] + 0.0722 * mean[2];
                            let sq_lum = 0.2126 * sum_sq[0] / count
                                + 0.7152 * sum_sq[1] / count
                                + 0.0722 * sum_sq[2] / count;
                            let var = sq_lum - mean_lum * mean_lum;
                            if var < best_var {
                                best_var = var;
                                best_mean = mean;
                            }
                        }
                        [
                            best_mean[0] as u8,
                            best_mean[1] as u8,
                            best_mean[2] as u8,
                            orig[3],
                        ]
                    })
                    .flat_map(|px| px)
                    .collect::<Vec<_>>()
            })
            .collect();

        let out = ImageBuffer::from_raw(w, h, raw).unwrap();
        DynamicImage::ImageRgba8(out)
    }

    fn params(&self) -> EffectParams {
        EffectParams::Kuwahara(self.0.clone())
    }
    fn set_params(&mut self, p: EffectParams) {
        if let EffectParams::Kuwahara(v) = p {
            self.0 = v;
        }
    }
    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(Kuwahara(self.0.clone()))
    }
}
