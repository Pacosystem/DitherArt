use super::{Effect, EffectParams};
use image::{DynamicImage, ImageBuffer};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NoiseParams {
    pub intensity: f32,
    pub seed: u64,
    pub mode: NoiseMode,
    pub monochrome: bool,
}

impl Default for NoiseParams {
    fn default() -> Self {
        Self {
            intensity: 0.1,
            seed: 42,
            mode: NoiseMode::Gaussian,
            monochrome: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum NoiseMode {
    Uniform,
    Gaussian,
}

// LCG per-pixel — deterministic, parallelizable
fn lcg(state: u64) -> u64 {
    state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407)
}

fn uniform_noise(seed: u64, idx: u64, channel: u64) -> f32 {
    let s = lcg(lcg(seed ^ idx) ^ channel);
    (s >> 33) as f32 / (u32::MAX as f32) * 2.0 - 1.0
}

fn gaussian_noise(seed: u64, idx: u64, channel: u64) -> f32 {
    // Box-Muller transform
    let s1 = lcg(lcg(seed ^ idx) ^ channel);
    let s2 = lcg(s1);
    let u1 = (s1 >> 33) as f32 / (u32::MAX as f32);
    let u2 = (s2 >> 33) as f32 / (u32::MAX as f32);
    let u1 = u1.max(1e-7);
    (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos()
}

pub struct Noise(pub NoiseParams);

impl Effect for Noise {
    fn name(&self) -> &str {
        "Noise"
    }

    fn apply(&self, img: &DynamicImage) -> DynamicImage {
        let p = &self.0;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        let scale = p.intensity * 128.0;
        let raw = rgba.into_raw();

        let out: Vec<u8> = (0..raw.len() / 4)
            .into_par_iter()
            .map(|i| {
                let idx = i as u64;
                let base = i * 4;
                let px = &raw[base..base + 4];
                let get_noise = |ch: u64| -> f32 {
                    match p.mode {
                        NoiseMode::Uniform => uniform_noise(p.seed, idx, ch),
                        NoiseMode::Gaussian => {
                            gaussian_noise(p.seed, idx, ch).clamp(-3.0, 3.0) / 3.0
                        }
                    }
                };
                let luma_noise = get_noise(0) * scale;
                let r = (px[0] as f32
                    + if p.monochrome {
                        luma_noise
                    } else {
                        get_noise(0) * scale
                    })
                .clamp(0.0, 255.0) as u8;
                let g = (px[1] as f32
                    + if p.monochrome {
                        luma_noise
                    } else {
                        get_noise(1) * scale
                    })
                .clamp(0.0, 255.0) as u8;
                let b = (px[2] as f32
                    + if p.monochrome {
                        luma_noise
                    } else {
                        get_noise(2) * scale
                    })
                .clamp(0.0, 255.0) as u8;
                [r, g, b, px[3]]
            })
            .flat_map(|px| px)
            .collect();

        let out = ImageBuffer::from_raw(w, h, out).unwrap();
        DynamicImage::ImageRgba8(out)
    }

    fn params(&self) -> EffectParams {
        EffectParams::Noise(self.0.clone())
    }
    fn set_params(&mut self, p: EffectParams) {
        if let EffectParams::Noise(v) = p {
            self.0 = v;
        }
    }
    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(Noise(self.0.clone()))
    }
}
