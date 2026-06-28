use image::{DynamicImage, ImageBuffer, Rgba};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use super::{Effect, EffectParams};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DitherParams {
    pub algorithm: DitherAlgorithm,
    pub palette: DitherPalette,
    pub scale_factor: u32,
    pub bayer_size: BayerSize,
    pub custom_colors: Vec<[u8; 3]>,
    /// After dithering, replace bright (white) pixels with the original image color
    pub color_preserve: bool,
}

impl Default for DitherParams {
    fn default() -> Self {
        Self {
            algorithm: DitherAlgorithm::FloydSteinberg,
            palette: DitherPalette::BW,
            scale_factor: 1,
            bayer_size: BayerSize::B4,
            custom_colors: vec![[0, 0, 0], [255, 255, 255]],
            color_preserve: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum DitherAlgorithm {
    FloydSteinberg,
    Atkinson,
    Ordered,
    Random,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum DitherPalette {
    BW,
    Color4,
    Color8,
    Color16,
    Custom,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum BayerSize {
    B2,
    B4,
    B8,
}

fn build_palette(p: &DitherParams) -> Vec<[u8; 3]> {
    match p.palette {
        DitherPalette::BW => vec![[0, 0, 0], [255, 255, 255]],
        DitherPalette::Color4 => vec![
            [0, 0, 0], [255, 0, 0], [0, 255, 0], [0, 0, 255],
        ],
        DitherPalette::Color8 => vec![
            [0, 0, 0], [255, 0, 0], [0, 255, 0], [0, 0, 255],
            [255, 255, 0], [0, 255, 255], [255, 0, 255], [255, 255, 255],
        ],
        DitherPalette::Color16 => {
            let mut v = vec![];
            for r in [0u8, 128, 255] {
                for g in [0u8, 128, 255] {
                    for b in [0u8, 128, 255] {
                        if v.len() < 16 { v.push([r, g, b]); }
                    }
                }
            }
            v
        }
        DitherPalette::Custom => p.custom_colors.clone(),
    }
}

fn nearest_color(palette: &[[u8; 3]], r: f32, g: f32, b: f32) -> [u8; 3] {
    palette.iter().min_by_key(|c| {
        let dr = r - c[0] as f32;
        let dg = g - c[1] as f32;
        let db = b - c[2] as f32;
        ((dr * dr + dg * dg + db * db) * 1000.0) as i64
    }).copied().unwrap_or([0, 0, 0])
}

// Inline error distribution — no closure, no clone, just direct mutation
#[inline(always)]
fn distribute(buf: &mut [[f32; 3]], w: u32, h: u32, tx: i32, ty: i32, er: f32, eg: f32, eb: f32, factor: f32) {
    if tx >= 0 && tx < w as i32 && ty >= 0 && ty < h as i32 {
        let j = (ty as u32 * w + tx as u32) as usize;
        buf[j][0] = (buf[j][0] + er * factor).clamp(0.0, 255.0);
        buf[j][1] = (buf[j][1] + eg * factor).clamp(0.0, 255.0);
        buf[j][2] = (buf[j][2] + eb * factor).clamp(0.0, 255.0);
    }
}

const BAYER2: [[f32; 2]; 2] = [[0.0, 2.0], [3.0, 1.0]];
const BAYER4: [[f32; 4]; 4] = [
    [0.0, 8.0, 2.0, 10.0],
    [12.0, 4.0, 14.0, 6.0],
    [3.0, 11.0, 1.0, 9.0],
    [15.0, 7.0, 13.0, 5.0],
];
const BAYER8: [[f32; 8]; 8] = [
    [0.0, 32.0, 8.0, 40.0, 2.0, 34.0, 10.0, 42.0],
    [48.0, 16.0, 56.0, 24.0, 50.0, 18.0, 58.0, 26.0],
    [12.0, 44.0, 4.0, 36.0, 14.0, 46.0, 6.0, 38.0],
    [60.0, 28.0, 52.0, 20.0, 62.0, 30.0, 54.0, 22.0],
    [3.0, 35.0, 11.0, 43.0, 1.0, 33.0, 9.0, 41.0],
    [51.0, 19.0, 59.0, 27.0, 49.0, 17.0, 57.0, 25.0],
    [15.0, 47.0, 7.0, 39.0, 13.0, 45.0, 5.0, 37.0],
    [63.0, 31.0, 55.0, 23.0, 61.0, 29.0, 53.0, 21.0],
];

fn bayer_threshold(x: u32, y: u32, size: &BayerSize) -> f32 {
    match size {
        BayerSize::B2 => BAYER2[(y % 2) as usize][(x % 2) as usize] / 4.0,
        BayerSize::B4 => BAYER4[(y % 4) as usize][(x % 4) as usize] / 16.0,
        BayerSize::B8 => BAYER8[(y % 8) as usize][(x % 8) as usize] / 64.0,
    }
}

fn apply_dither(img: &image::ImageBuffer<Rgba<u8>, Vec<u8>>, params: &DitherParams, original: Option<&image::ImageBuffer<Rgba<u8>, Vec<u8>>>) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    let (w, h) = img.dimensions();
    let palette = build_palette(params);
    // Store original colors before modifying buf (needed for color_preserve)
    let original_rgb: Vec<[f32; 3]> = if params.color_preserve {
        original.unwrap_or(img).pixels().map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect()
    } else {
        vec![]
    };
    let mut buf: Vec<[f32; 3]> = img.pixels().map(|p| {
        [p[0] as f32, p[1] as f32, p[2] as f32]
    }).collect();
    let alpha: Vec<u8> = img.pixels().map(|p| p[3]).collect();

    match params.algorithm {
        DitherAlgorithm::FloydSteinberg => {
            for y in 0..h {
                for x in 0..w {
                    let i = (y * w + x) as usize;
                    let [r, g, b] = buf[i];
                    let closest = nearest_color(&palette, r, g, b);
                    buf[i] = [closest[0] as f32, closest[1] as f32, closest[2] as f32];
                    let er = r - closest[0] as f32;
                    let eg = g - closest[1] as f32;
                    let eb = b - closest[2] as f32;
                    // Distribute error in-place — no clone
                    distribute(&mut buf, w, h, x as i32 + 1, y as i32,     er, eg, eb, 7.0/16.0);
                    distribute(&mut buf, w, h, x as i32 - 1, y as i32 + 1, er, eg, eb, 3.0/16.0);
                    distribute(&mut buf, w, h, x as i32,     y as i32 + 1, er, eg, eb, 5.0/16.0);
                    distribute(&mut buf, w, h, x as i32 + 1, y as i32 + 1, er, eg, eb, 1.0/16.0);
                }
            }
        }
        DitherAlgorithm::Atkinson => {
            for y in 0..h {
                for x in 0..w {
                    let i = (y * w + x) as usize;
                    let [r, g, b] = buf[i];
                    let closest = nearest_color(&palette, r, g, b);
                    buf[i] = [closest[0] as f32, closest[1] as f32, closest[2] as f32];
                    let er = (r - closest[0] as f32) / 8.0;
                    let eg = (g - closest[1] as f32) / 8.0;
                    let eb = (b - closest[2] as f32) / 8.0;
                    for (tx, ty) in [
                        (x as i32 + 1, y as i32),
                        (x as i32 + 2, y as i32),
                        (x as i32 - 1, y as i32 + 1),
                        (x as i32,     y as i32 + 1),
                        (x as i32 + 1, y as i32 + 1),
                        (x as i32,     y as i32 + 2),
                    ] {
                        distribute(&mut buf, w, h, tx, ty, er, eg, eb, 1.0);
                    }
                }
            }
        }
        DitherAlgorithm::Ordered => {
            // Fully parallelizable — no error propagation
            buf.par_iter_mut().enumerate().for_each(|(i, px)| {
                let x = (i as u32) % w;
                let y = (i as u32) / w;
                let t = bayer_threshold(x, y, &params.bayer_size) - 0.5;
                let r = (px[0] + t * 255.0).clamp(0.0, 255.0);
                let g = (px[1] + t * 255.0).clamp(0.0, 255.0);
                let b = (px[2] + t * 255.0).clamp(0.0, 255.0);
                let closest = nearest_color(&palette, r, g, b);
                *px = [closest[0] as f32, closest[1] as f32, closest[2] as f32];
            });
        }
        DitherAlgorithm::Random => {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            buf.par_iter_mut().enumerate().for_each(|(i, px)| {
                let x = (i as u32) % w;
                let y = (i as u32) / w;
                let mut hasher = DefaultHasher::new();
                (x, y, 0xdeadbeef_u32).hash(&mut hasher);
                let noise = (hasher.finish() % 256) as f32 / 255.0 - 0.5;
                let r = (px[0] + noise * 128.0).clamp(0.0, 255.0);
                let g = (px[1] + noise * 128.0).clamp(0.0, 255.0);
                let b = (px[2] + noise * 128.0).clamp(0.0, 255.0);
                let closest = nearest_color(&palette, r, g, b);
                *px = [closest[0] as f32, closest[1] as f32, closest[2] as f32];
            });
        }
    }

    // Color preserve: where dithered pixel is bright, restore the original color
    if params.color_preserve && !original_rgb.is_empty() {
        for (i, px) in buf.iter_mut().enumerate() {
            let lum = px[0] * 0.2126 + px[1] * 0.7152 + px[2] * 0.0722;
            if lum > 127.0 {
                *px = original_rgb[i];
            }
        }
    }

    let mut out = ImageBuffer::new(w, h);
    for (i, px) in buf.iter().enumerate() {
        let x = (i as u32) % w;
        let y = (i as u32) / w;
        out.put_pixel(x, y, Rgba([px[0] as u8, px[1] as u8, px[2] as u8, alpha[i]]));
    }
    out
}

pub struct Dither(pub DitherParams);

impl Effect for Dither {
    fn name(&self) -> &str { "Dither" }

    fn apply(&self, img: &DynamicImage) -> DynamicImage {
        let p = &self.0;
        let scale = p.scale_factor.max(1);
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();

        if scale <= 1 {
            return DynamicImage::ImageRgba8(apply_dither(&rgba, p, None));
        }

        let small_w = (w / scale).max(1);
        let small_h = (h / scale).max(1);
        let small = image::imageops::resize(&rgba, small_w, small_h, image::imageops::FilterType::Nearest);
        // For color_preserve at scaled resolution, also downscale the original
        let small_orig = if p.color_preserve {
            Some(image::imageops::resize(&rgba, small_w, small_h, image::imageops::FilterType::Triangle))
        } else {
            None
        };
        let dithered = apply_dither(&small, p, small_orig.as_ref());
        let upscaled = image::imageops::resize(&dithered, w, h, image::imageops::FilterType::Nearest);
        DynamicImage::ImageRgba8(upscaled)
    }

    fn params(&self) -> EffectParams { EffectParams::Dither(self.0.clone()) }
    fn set_params(&mut self, params: EffectParams) {
        if let EffectParams::Dither(p) = params { self.0 = p; }
    }
    fn clone_box(&self) -> Box<dyn Effect> { Box::new(Dither(self.0.clone())) }
}
