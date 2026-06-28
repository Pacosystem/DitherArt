use super::{luminance, Effect, EffectParams};
use image::{DynamicImage, ImageBuffer, Rgba};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BloomParams {
    pub threshold: f32,
    pub intensity: f32,
    pub radius: f32,
}

impl Default for BloomParams {
    fn default() -> Self {
        Self {
            threshold: 0.7,
            intensity: 0.8,
            radius: 15.0,
        }
    }
}

// Separable Gaussian blur — horizontal pass
fn blur_h(img: &ImageBuffer<Rgba<u8>, Vec<u8>>, sigma: f32) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    let (w, h) = img.dimensions();
    let r = (sigma * 3.0).ceil() as i32;
    let kernel: Vec<f32> = {
        let mut k: Vec<f32> = (-r..=r)
            .map(|x| (-(x * x) as f32 / (2.0 * sigma * sigma)).exp())
            .collect();
        let s: f32 = k.iter().sum();
        k.iter_mut().for_each(|v| *v /= s);
        k
    };
    let rows: Vec<Vec<Rgba<u8>>> = (0..h)
        .into_par_iter()
        .map(|y| {
            (0..w)
                .map(|x| {
                    let mut acc = [0f32; 4];
                    for (ki, &kv) in kernel.iter().enumerate() {
                        let sx = (x as i32 + ki as i32 - r).clamp(0, w as i32 - 1) as u32;
                        let px = img.get_pixel(sx, y);
                        for c in 0..4 {
                            acc[c] += px[c] as f32 * kv;
                        }
                    }
                    Rgba(acc.map(|v| v as u8))
                })
                .collect()
        })
        .collect();
    let mut out = ImageBuffer::new(w, h);
    for (y, row) in rows.iter().enumerate() {
        for (x, px) in row.iter().enumerate() {
            out.put_pixel(x as u32, y as u32, *px);
        }
    }
    out
}

// Vertical pass
fn blur_v(img: &ImageBuffer<Rgba<u8>, Vec<u8>>, sigma: f32) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    let (w, h) = img.dimensions();
    let r = (sigma * 3.0).ceil() as i32;
    let kernel: Vec<f32> = {
        let mut k: Vec<f32> = (-r..=r)
            .map(|x| (-(x * x) as f32 / (2.0 * sigma * sigma)).exp())
            .collect();
        let s: f32 = k.iter().sum();
        k.iter_mut().for_each(|v| *v /= s);
        k
    };
    let cols: Vec<Vec<Rgba<u8>>> = (0..w)
        .into_par_iter()
        .map(|x| {
            (0..h)
                .map(|y| {
                    let mut acc = [0f32; 4];
                    for (ki, &kv) in kernel.iter().enumerate() {
                        let sy = (y as i32 + ki as i32 - r).clamp(0, h as i32 - 1) as u32;
                        let px = img.get_pixel(x, sy);
                        for c in 0..4 {
                            acc[c] += px[c] as f32 * kv;
                        }
                    }
                    Rgba(acc.map(|v| v as u8))
                })
                .collect()
        })
        .collect();
    let mut out = ImageBuffer::new(w, h);
    for (x, col) in cols.iter().enumerate() {
        for (y, px) in col.iter().enumerate() {
            out.put_pixel(x as u32, y as u32, *px);
        }
    }
    out
}

pub struct Bloom(pub BloomParams);

impl Effect for Bloom {
    fn name(&self) -> &str {
        "Bloom"
    }

    fn apply(&self, img: &DynamicImage) -> DynamicImage {
        let p = &self.0;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();

        // Extract bright pixels above threshold
        let mut bright: ImageBuffer<Rgba<u8>, _> = ImageBuffer::new(w, h);
        for (x, y, px) in rgba.enumerate_pixels() {
            let lum = luminance(px[0], px[1], px[2]);
            let t = (lum - p.threshold).max(0.0) / (1.0 - p.threshold).max(0.01);
            bright.put_pixel(
                x,
                y,
                Rgba([
                    (px[0] as f32 * t) as u8,
                    (px[1] as f32 * t) as u8,
                    (px[2] as f32 * t) as u8,
                    px[3],
                ]),
            );
        }

        // Gaussian blur the bright mask (separable)
        let sigma = p.radius.max(0.5);
        let blurred = blur_v(&blur_h(&bright, sigma), sigma);

        // Screen blend: out = 1 - (1 - base) * (1 - bloom * intensity)
        let mut out: ImageBuffer<Rgba<u8>, _> = ImageBuffer::new(w, h);
        for (x, y, px) in rgba.enumerate_pixels() {
            let bl = blurred.get_pixel(x, y);
            let blend = |base: u8, bloom: u8| -> u8 {
                let b = base as f32 / 255.0;
                let bl = bloom as f32 / 255.0 * p.intensity;
                ((1.0 - (1.0 - b) * (1.0 - bl)).clamp(0.0, 1.0) * 255.0) as u8
            };
            out.put_pixel(
                x,
                y,
                Rgba([
                    blend(px[0], bl[0]),
                    blend(px[1], bl[1]),
                    blend(px[2], bl[2]),
                    px[3],
                ]),
            );
        }
        DynamicImage::ImageRgba8(out)
    }

    fn params(&self) -> EffectParams {
        EffectParams::Bloom(self.0.clone())
    }
    fn set_params(&mut self, p: EffectParams) {
        if let EffectParams::Bloom(v) = p {
            self.0 = v;
        }
    }
    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(Bloom(self.0.clone()))
    }
}
