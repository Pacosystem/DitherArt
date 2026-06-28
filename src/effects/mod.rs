pub mod bloom;
pub mod chromatic;
pub mod color_adjust;
pub mod databend;
pub mod dither;
pub mod halftone;
pub mod kuwahara;
pub mod noise;
pub mod pixel_sort;
pub mod pixelate;
pub mod quantize;
pub mod transform;
pub mod vhs;

use image::DynamicImage;
use serde::{Deserialize, Serialize};

#[allow(dead_code)]
pub trait Effect: Send + Sync {
    fn name(&self) -> &str;
    fn apply(&self, img: &DynamicImage) -> DynamicImage;
    fn params(&self) -> EffectParams;
    fn set_params(&mut self, params: EffectParams);
    fn clone_box(&self) -> Box<dyn Effect>;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EffectParams {
    PixelSort(pixel_sort::PixelSortParams),
    Dither(dither::DitherParams),
    Quantize(quantize::QuantizeParams),
    Databend(databend::DatabendParams),
    Halftone(halftone::HalftoneParams),
    ChromaticAberration(chromatic::ChromaticParams),
    ColorAdjust(color_adjust::ColorAdjustParams),
    Transform(transform::TransformParams),
    Pixelate(pixelate::PixelateParams),
    Noise(noise::NoiseParams),
    Bloom(bloom::BloomParams),
    Kuwahara(kuwahara::KuwaharaParams),
    Vhs(vhs::VhsParams),
}

impl EffectParams {
    pub fn type_name(&self) -> &str {
        match self {
            EffectParams::PixelSort(_) => "Pixel Sort",
            EffectParams::Dither(_) => "Dither",
            EffectParams::Quantize(_) => "Color Quantization",
            EffectParams::Databend(_) => "Databend",
            EffectParams::Halftone(_) => "Halftone",
            EffectParams::ChromaticAberration(_) => "Chromatic Aberration",
            EffectParams::ColorAdjust(_) => "Color Adjust",
            EffectParams::Transform(_) => "Transform",
            EffectParams::Pixelate(_) => "Pixelate",
            EffectParams::Noise(_) => "Noise",
            EffectParams::Bloom(_) => "Bloom",
            EffectParams::Kuwahara(_) => "Kuwahara",
            EffectParams::Vhs(_) => "VHS / CRT",
        }
    }
}

impl EffectParams {
    /// Returns (key, current_value, range_min, range_max) for every animatable numeric param.
    pub fn animatable_params(&self) -> Vec<(&'static str, f32, f32, f32)> {
        match self {
            EffectParams::ColorAdjust(p) => vec![
                ("brightness", p.brightness, -1.0, 1.0),
                ("contrast", p.contrast, -1.0, 1.0),
                ("saturation", p.saturation, -1.0, 1.0),
                ("hue_shift", p.hue_shift, -180.0, 180.0),
                ("temperature", p.temperature, -1.0, 1.0),
            ],
            EffectParams::PixelSort(p) => vec![
                ("threshold_low", p.threshold_low, 0.0, 1.0),
                ("threshold_high", p.threshold_high, 0.0, 1.0),
            ],
            EffectParams::ChromaticAberration(p) => vec![
                ("r_dx", p.r_dx as f32, -100.0, 100.0),
                ("r_dy", p.r_dy as f32, -100.0, 100.0),
                ("b_dx", p.b_dx as f32, -100.0, 100.0),
                ("b_dy", p.b_dy as f32, -100.0, 100.0),
                ("radial_amount", p.radial_amount, 0.0, 0.5),
            ],
            EffectParams::Halftone(p) => vec![
                ("dot_size", p.dot_size, 1.0, 50.0),
                ("spacing", p.spacing, 1.0, 60.0),
                ("angle", p.angle, 0.0, 90.0),
            ],
            EffectParams::Databend(p) => vec![("intensity", p.intensity, 0.0, 1.0)],
            EffectParams::Dither(_) | EffectParams::Quantize(_) => vec![],
            EffectParams::Transform(p) => vec![
                ("angle", p.angle, -180.0, 180.0),
                ("crop_x", p.crop_x, 0.0, 99.0),
                ("crop_y", p.crop_y, 0.0, 99.0),
                ("crop_w", p.crop_w, 1.0, 100.0),
                ("crop_h", p.crop_h, 1.0, 100.0),
            ],
            EffectParams::Bloom(p) => vec![
                ("threshold", p.threshold, 0.0, 1.0),
                ("intensity", p.intensity, 0.0, 3.0),
                ("radius", p.radius, 0.5, 50.0),
            ],
            EffectParams::Noise(p) => vec![("intensity", p.intensity, 0.0, 1.0)],
            EffectParams::Vhs(p) => vec![
                ("scanline_intensity", p.scanline_intensity, 0.0, 1.0),
                ("color_bleed", p.color_bleed, 0.0, 20.0),
                ("noise", p.noise, 0.0, 0.5),
                ("vignette", p.vignette, 0.0, 1.0),
                ("barrel", p.barrel, 0.0, 0.3),
                ("tracking_distortion", p.tracking_distortion, 0.0, 1.0),
            ],
            EffectParams::Pixelate(p) => vec![
                ("block_w", p.block_w as f32, 2.0, 100.0),
                ("block_h", p.block_h as f32, 2.0, 100.0),
            ],
            EffectParams::Kuwahara(_) => vec![],
        }
    }

    pub fn set_param(&mut self, key: &str, value: f32) {
        match self {
            EffectParams::ColorAdjust(p) => match key {
                "brightness" => p.brightness = value.clamp(-1.0, 1.0),
                "contrast" => p.contrast = value.clamp(-1.0, 1.0),
                "saturation" => p.saturation = value.clamp(-1.0, 1.0),
                "hue_shift" => p.hue_shift = value.clamp(-180.0, 180.0),
                "temperature" => p.temperature = value.clamp(-1.0, 1.0),
                _ => {}
            },
            EffectParams::PixelSort(p) => match key {
                "threshold_low" => p.threshold_low = value.clamp(0.0, 1.0),
                "threshold_high" => p.threshold_high = value.clamp(0.0, 1.0),
                _ => {}
            },
            EffectParams::ChromaticAberration(p) => match key {
                "r_dx" => p.r_dx = value as i32,
                "r_dy" => p.r_dy = value as i32,
                "b_dx" => p.b_dx = value as i32,
                "b_dy" => p.b_dy = value as i32,
                "radial_amount" => p.radial_amount = value.clamp(0.0, 0.5),
                _ => {}
            },
            EffectParams::Halftone(p) => match key {
                "dot_size" => p.dot_size = value.clamp(1.0, 50.0),
                "spacing" => p.spacing = value.clamp(1.0, 60.0),
                "angle" => p.angle = value.clamp(0.0, 90.0),
                _ => {}
            },
            EffectParams::Databend(p) => match key {
                "intensity" => p.intensity = value.clamp(0.0, 1.0),
                _ => {}
            },
            EffectParams::Transform(p) => match key {
                "angle" => p.angle = value.clamp(-180.0, 180.0),
                "crop_x" => p.crop_x = value.clamp(0.0, 99.0),
                "crop_y" => p.crop_y = value.clamp(0.0, 99.0),
                "crop_w" => p.crop_w = value.clamp(1.0, 100.0),
                "crop_h" => p.crop_h = value.clamp(1.0, 100.0),
                _ => {}
            },
            EffectParams::Bloom(p) => match key {
                "threshold" => p.threshold = value.clamp(0.0, 1.0),
                "intensity" => p.intensity = value.clamp(0.0, 3.0),
                "radius" => p.radius = value.clamp(0.5, 50.0),
                _ => {}
            },
            EffectParams::Noise(p) => match key {
                "intensity" => p.intensity = value.clamp(0.0, 1.0),
                _ => {}
            },
            EffectParams::Vhs(p) => match key {
                "scanline_intensity" => p.scanline_intensity = value.clamp(0.0, 1.0),
                "color_bleed" => p.color_bleed = value.clamp(0.0, 20.0),
                "noise" => p.noise = value.clamp(0.0, 0.5),
                "vignette" => p.vignette = value.clamp(0.0, 1.0),
                "barrel" => p.barrel = value.clamp(0.0, 0.3),
                "tracking_distortion" => p.tracking_distortion = value.clamp(0.0, 1.0),
                _ => {}
            },
            EffectParams::Pixelate(p) => match key {
                "block_w" => p.block_w = value.clamp(2.0, 100.0) as u32,
                "block_h" => p.block_h = value.clamp(2.0, 100.0) as u32,
                _ => {}
            },
            EffectParams::Kuwahara(_) => {}
            EffectParams::Dither(_) | EffectParams::Quantize(_) => {}
        }
    }
}

pub fn make_effect(params: EffectParams) -> Box<dyn Effect> {
    match params {
        EffectParams::PixelSort(p) => Box::new(pixel_sort::PixelSort(p)),
        EffectParams::Dither(p) => Box::new(dither::Dither(p)),
        EffectParams::Quantize(p) => Box::new(quantize::Quantize(p)),
        EffectParams::Databend(p) => Box::new(databend::Databend(p)),
        EffectParams::Halftone(p) => Box::new(halftone::Halftone(p)),
        EffectParams::ChromaticAberration(p) => Box::new(chromatic::ChromaticAberration(p)),
        EffectParams::ColorAdjust(p) => Box::new(color_adjust::ColorAdjust(p)),
        EffectParams::Transform(p) => Box::new(transform::Transform(p)),
        EffectParams::Pixelate(p) => Box::new(pixelate::Pixelate(p)),
        EffectParams::Noise(p) => Box::new(noise::Noise(p)),
        EffectParams::Bloom(p) => Box::new(bloom::Bloom(p)),
        EffectParams::Kuwahara(p) => Box::new(kuwahara::Kuwahara(p)),
        EffectParams::Vhs(p) => Box::new(vhs::Vhs(p)),
    }
}

pub fn luminance(r: u8, g: u8, b: u8) -> f32 {
    0.2126 * (r as f32 / 255.0) + 0.7152 * (g as f32 / 255.0) + 0.0722 * (b as f32 / 255.0)
}

pub fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-6 {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        ((g - b) / d + if g < b { 6.0 } else { 0.0 }) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    (h, s, l)
}
