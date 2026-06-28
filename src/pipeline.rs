use crate::effects::{make_effect, EffectParams};
use crate::gpu::GpuCompute;
use image::DynamicImage;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct PipelineStep {
    pub enabled: bool,
    pub opacity: f32,
    pub params: EffectParams,
}

impl PipelineStep {
    pub fn new(params: EffectParams) -> Self {
        Self {
            enabled: true,
            opacity: 1.0,
            params,
        }
    }
}

pub struct Pipeline {
    pub steps: Vec<(bool, f32, Box<dyn crate::effects::Effect>)>,
}

impl Pipeline {
    pub fn from_steps(steps: &[PipelineStep]) -> Self {
        Self {
            steps: steps
                .iter()
                .map(|s| (s.enabled, s.opacity, make_effect(s.params.clone())))
                .collect(),
        }
    }

    pub fn run(&self, source: &DynamicImage, gpu: Option<&GpuCompute>) -> DynamicImage {
        let mut img = source.clone();
        for (enabled, opacity, effect) in &self.steps {
            if !enabled {
                continue;
            }

            let pre = img.clone();

            // Route GPU-friendly effects to the compute path when available
            img = if let Some(gpu) = gpu {
                match effect.params() {
                    EffectParams::ColorAdjust(p) => gpu.color_adjust(&img, &p),
                    EffectParams::ChromaticAberration(p)
                        if matches!(p.mode, crate::effects::chromatic::ChromaticMode::Fixed) =>
                    {
                        gpu.chromatic_fixed(&img, &p)
                    }
                    _ => effect.apply(&img),
                }
            } else {
                effect.apply(&img)
            };

            // Blend with pre-effect image at opacity
            if *opacity < 0.999 {
                img = blend(&pre, &img, *opacity);
            }
        }
        img
    }
}

/// Alpha-blend two images: result = base * (1-t) + over * t
fn blend(base: &DynamicImage, over: &DynamicImage, t: f32) -> DynamicImage {
    let base_rgba = base.to_rgba8();
    let over_rgba = over.to_rgba8();
    let (w, h) = base_rgba.dimensions();

    let base_raw = base_rgba.as_raw();
    let over_raw = over_rgba.as_raw();
    let inv_t = 1.0 - t;

    let blended: Vec<u8> = (0..h)
        .into_par_iter()
        .flat_map(|y| {
            let row_start = (y * w) as usize;
            let row_end = row_start + w as usize;
            let mut row = Vec::with_capacity(w as usize * 4);
            for idx in row_start..row_end {
                let i4 = idx * 4;
                row.push((base_raw[i4] as f32 * inv_t + over_raw[i4] as f32 * t) as u8);
                row.push((base_raw[i4 + 1] as f32 * inv_t + over_raw[i4 + 1] as f32 * t) as u8);
                row.push((base_raw[i4 + 2] as f32 * inv_t + over_raw[i4 + 2] as f32 * t) as u8);
                row.push((base_raw[i4 + 3] as f32 * inv_t + over_raw[i4 + 3] as f32 * t) as u8);
            }
            row
        })
        .collect();

    let out = image::ImageBuffer::from_raw(w, h, blended).unwrap();
    DynamicImage::ImageRgba8(out)
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Preset {
    pub steps: Vec<PipelineStep>,
}
