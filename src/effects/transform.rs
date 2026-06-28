use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use super::{Effect, EffectParams};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransformParams {
    pub rotation: RotationMode,
    pub angle: f32,
    pub expand_canvas: bool,
    pub flip_h: bool,
    pub flip_v: bool,
    pub crop_enabled: bool,
    pub crop_x: f32,
    pub crop_y: f32,
    pub crop_w: f32,
    pub crop_h: f32,
}

impl Default for TransformParams {
    fn default() -> Self {
        Self {
            rotation: RotationMode::None,
            angle: 0.0,
            expand_canvas: true,
            flip_h: false,
            flip_v: false,
            crop_enabled: false,
            crop_x: 0.0,
            crop_y: 0.0,
            crop_w: 100.0,
            crop_h: 100.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum RotationMode {
    None,
    Rotate90,
    Rotate180,
    Rotate270,
    Arbitrary,
}

fn rotate_arbitrary(img: &DynamicImage, angle_deg: f32, expand: bool) -> DynamicImage {
    let rgba = img.to_rgba8();
    let (iw, ih) = rgba.dimensions();
    let theta = angle_deg.to_radians();
    let cos_t = theta.cos();
    let sin_t = theta.sin();

    let (ow, oh) = if expand {
        let w = (iw as f32 * cos_t.abs() + ih as f32 * sin_t.abs()).ceil() as u32;
        let h = (iw as f32 * sin_t.abs() + ih as f32 * cos_t.abs()).ceil() as u32;
        (w.max(1), h.max(1))
    } else {
        (iw, ih)
    };

    let cx_in  = iw as f32 / 2.0;
    let cy_in  = ih as f32 / 2.0;
    let cx_out = ow as f32 / 2.0;
    let cy_out = oh as f32 / 2.0;

    let pixels: Vec<Rgba<u8>> = (0..oh * ow)
        .into_par_iter()
        .map(|idx| {
            let ox = (idx % ow) as f32;
            let oy = (idx / ow) as f32;
            let dx = ox - cx_out;
            let dy = oy - cy_out;
            // Inverse rotation (output → input)
            let ix = dx * cos_t + dy * sin_t + cx_in;
            let iy = -dx * sin_t + dy * cos_t + cy_in;

            if ix < 0.0 || iy < 0.0 || ix >= iw as f32 || iy >= ih as f32 {
                Rgba([0, 0, 0, 0])
            } else {
                // Bilinear interpolation
                let x0 = ix.floor() as u32;
                let y0 = iy.floor() as u32;
                let x1 = (x0 + 1).min(iw - 1);
                let y1 = (y0 + 1).min(ih - 1);
                let fx = ix - ix.floor();
                let fy = iy - iy.floor();
                let p00 = rgba.get_pixel(x0, y0);
                let p10 = rgba.get_pixel(x1, y0);
                let p01 = rgba.get_pixel(x0, y1);
                let p11 = rgba.get_pixel(x1, y1);
                let ch = |c: usize| -> u8 {
                    (p00[c] as f32 * (1.0-fx) * (1.0-fy)
                   + p10[c] as f32 * fx * (1.0-fy)
                   + p01[c] as f32 * (1.0-fx) * fy
                   + p11[c] as f32 * fx * fy) as u8
                };
                Rgba([ch(0), ch(1), ch(2), ch(3)])
            }
        })
        .collect();

    let mut out: ImageBuffer<Rgba<u8>, _> = ImageBuffer::new(ow, oh);
    for (i, px) in pixels.iter().enumerate() {
        out.put_pixel((i as u32) % ow, (i as u32) / ow, *px);
    }
    DynamicImage::ImageRgba8(out)
}

pub struct Transform(pub TransformParams);

impl Effect for Transform {
    fn name(&self) -> &str { "Transform" }

    fn apply(&self, img: &DynamicImage) -> DynamicImage {
        let p = &self.0;
        let mut result = img.clone();

        // 1. Rotation
        result = match p.rotation {
            RotationMode::None      => result,
            RotationMode::Rotate90  => DynamicImage::ImageRgba8(image::imageops::rotate90(&result.to_rgba8())),
            RotationMode::Rotate180 => DynamicImage::ImageRgba8(image::imageops::rotate180(&result.to_rgba8())),
            RotationMode::Rotate270 => DynamicImage::ImageRgba8(image::imageops::rotate270(&result.to_rgba8())),
            RotationMode::Arbitrary => rotate_arbitrary(&result, p.angle, p.expand_canvas),
        };

        // 2. Flip
        if p.flip_h {
            result = DynamicImage::ImageRgba8(image::imageops::flip_horizontal(&result.to_rgba8()));
        }
        if p.flip_v {
            result = DynamicImage::ImageRgba8(image::imageops::flip_vertical(&result.to_rgba8()));
        }

        // 3. Crop (percentage-based so it's resolution-independent)
        if p.crop_enabled {
            let (w, h) = result.dimensions();
            let cx = (w as f32 * p.crop_x / 100.0) as u32;
            let cy = (h as f32 * p.crop_y / 100.0) as u32;
            let cw = ((w as f32 * p.crop_w / 100.0) as u32).max(1).min(w.saturating_sub(cx));
            let ch = ((h as f32 * p.crop_h / 100.0) as u32).max(1).min(h.saturating_sub(cy));
            result = result.crop_imm(cx, cy, cw, ch);
        }

        result
    }

    fn params(&self) -> EffectParams { EffectParams::Transform(self.0.clone()) }
    fn set_params(&mut self, params: EffectParams) {
        if let EffectParams::Transform(p) = params { self.0 = p; }
    }
    fn clone_box(&self) -> Box<dyn Effect> { Box::new(Transform(self.0.clone())) }
}
