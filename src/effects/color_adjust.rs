use image::{DynamicImage, ImageBuffer, Rgba};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use super::{Effect, EffectParams};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ColorAdjustParams {
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub hue_shift: f32,
    pub temperature: f32,
    pub invert: bool,
    pub sepia: bool,
    pub grayscale: bool,
}

impl Default for ColorAdjustParams {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 0.0,
            saturation: 0.0,
            hue_shift: 0.0,
            temperature: 0.0,
            invert: false,
            sepia: false,
            grayscale: false,
        }
    }
}

fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min) < 1e-6 {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        ((g - b) / d + if g < b { 6.0 } else { 0.0 }) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    (h, s, l)
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 { t += 1.0; }
    if t > 1.0 { t -= 1.0; }
    if t < 1.0/6.0 { return p + (q - p) * 6.0 * t; }
    if t < 1.0/2.0 { return q; }
    if t < 2.0/3.0 { return p + (q - p) * (2.0/3.0 - t) * 6.0; }
    p
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s < 1e-6 {
        return (l, l, l);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    (hue_to_rgb(p, q, h + 1.0/3.0), hue_to_rgb(p, q, h), hue_to_rgb(p, q, h - 1.0/3.0))
}

fn apply_contrast(v: f32, contrast: f32) -> f32 {
    let factor = (259.0 * (contrast * 255.0 + 255.0)) / (255.0 * (259.0 - contrast * 255.0));
    ((factor * (v - 0.5) + 0.5)).clamp(0.0, 1.0)
}

pub struct ColorAdjust(pub ColorAdjustParams);

impl Effect for ColorAdjust {
    fn name(&self) -> &str { "Color Adjust" }

    fn apply(&self, img: &DynamicImage) -> DynamicImage {
        let p = &self.0;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();

        let pixels: Vec<Rgba<u8>> = rgba.pixels().copied().collect();

        let processed: Vec<Rgba<u8>> = pixels.par_iter().map(|px| {
            let mut r = px[0] as f32 / 255.0;
            let mut g = px[1] as f32 / 255.0;
            let mut b = px[2] as f32 / 255.0;

            // Grayscale
            if p.grayscale || p.sepia {
                let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                r = lum; g = lum; b = lum;
            }

            // Sepia tint on top of grayscale
            if p.sepia {
                let sr = (r * 0.393 + g * 0.769 + b * 0.189).clamp(0.0, 1.0);
                let sg = (r * 0.349 + g * 0.686 + b * 0.168).clamp(0.0, 1.0);
                let sb = (r * 0.272 + g * 0.534 + b * 0.131).clamp(0.0, 1.0);
                r = sr; g = sg; b = sb;
            }

            // Hue shift + saturation via HSL
            if p.hue_shift.abs() > 0.001 || p.saturation.abs() > 0.001 {
                let (mut hue, mut sat, lum) = rgb_to_hsl(r, g, b);
                hue = (hue + p.hue_shift / 360.0).rem_euclid(1.0);
                sat = (sat + p.saturation).clamp(0.0, 1.0);
                let (nr, ng, nb) = hsl_to_rgb(hue, sat, lum);
                r = nr; g = ng; b = nb;
            }

            // Temperature (warm = more red/less blue, cool = less red/more blue)
            if p.temperature.abs() > 0.001 {
                let t = p.temperature * 0.2;
                r = (r + t).clamp(0.0, 1.0);
                b = (b - t).clamp(0.0, 1.0);
            }

            // Brightness
            if p.brightness.abs() > 0.001 {
                r = (r + p.brightness).clamp(0.0, 1.0);
                g = (g + p.brightness).clamp(0.0, 1.0);
                b = (b + p.brightness).clamp(0.0, 1.0);
            }

            // Contrast
            if p.contrast.abs() > 0.001 {
                r = apply_contrast(r, p.contrast);
                g = apply_contrast(g, p.contrast);
                b = apply_contrast(b, p.contrast);
            }

            // Invert
            if p.invert {
                r = 1.0 - r;
                g = 1.0 - g;
                b = 1.0 - b;
            }

            Rgba([(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8, px[3]])
        }).collect();

        let mut out = ImageBuffer::new(w, h);
        for (i, px) in processed.iter().enumerate() {
            out.put_pixel((i as u32) % w, (i as u32) / w, *px);
        }
        DynamicImage::ImageRgba8(out)
    }

    fn params(&self) -> EffectParams { EffectParams::ColorAdjust(self.0.clone()) }
    fn set_params(&mut self, params: EffectParams) {
        if let EffectParams::ColorAdjust(p) = params { self.0 = p; }
    }
    fn clone_box(&self) -> Box<dyn Effect> { Box::new(ColorAdjust(self.0.clone())) }
}
