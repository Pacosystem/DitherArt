use image::{DynamicImage, ImageBuffer, Rgba};
use serde::{Deserialize, Serialize};
use super::{Effect, EffectParams};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChromaticParams {
    pub r_dx: i32,
    pub r_dy: i32,
    pub g_dx: i32,
    pub g_dy: i32,
    pub b_dx: i32,
    pub b_dy: i32,
    pub mode: ChromaticMode,
    pub radial_amount: f32,
    pub angle: f32,
    pub scanline_horizontal_only: bool,
}

impl Default for ChromaticParams {
    fn default() -> Self {
        Self {
            r_dx: -5,
            r_dy: 0,
            g_dx: 0,
            g_dy: 0,
            b_dx: 5,
            b_dy: 0,
            mode: ChromaticMode::Fixed,
            radial_amount: 0.05,
            angle: 0.0,
            scanline_horizontal_only: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ChromaticMode {
    Fixed,
    Radial,
    Scanline,
}

pub struct ChromaticAberration(pub ChromaticParams);

impl Effect for ChromaticAberration {
    fn name(&self) -> &str { "Chromatic Aberration" }

    fn apply(&self, img: &DynamicImage) -> DynamicImage {
        let p = &self.0;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        let wf = w as f32;
        let hf = h as f32;
        let cx = wf / 2.0;
        let cy = hf / 2.0;

        let sample = |img: &image::ImageBuffer<Rgba<u8>, Vec<u8>>, x: i32, y: i32| -> Rgba<u8> {
            let x = x.clamp(0, w as i32 - 1) as u32;
            let y = y.clamp(0, h as i32 - 1) as u32;
            *img.get_pixel(x, y)
        };

        let mut out: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(w, h);

        for y in 0..h {
            for x in 0..w {
                let (rdx, rdy, gdx, gdy, bdx, bdy) = match p.mode {
                    ChromaticMode::Fixed => (p.r_dx, p.r_dy, p.g_dx, p.g_dy, p.b_dx, p.b_dy),
                    ChromaticMode::Radial => {
                        let dx = x as f32 - cx;
                        let dy = y as f32 - cy;
                        let dist = (dx * dx + dy * dy).sqrt() / (cx.hypot(cy));
                        let scale = dist * p.radial_amount;
                        let nx = dx / (cx.hypot(cy));
                        let ny = dy / (cx.hypot(cy));
                        let r_shift = scale * 1.0;
                        let b_shift = -scale * 1.0;
                        (
                            (nx * r_shift * w as f32) as i32,
                            (ny * r_shift * h as f32) as i32,
                            0, 0,
                            (nx * b_shift * w as f32) as i32,
                            (ny * b_shift * h as f32) as i32,
                        )
                    }
                    ChromaticMode::Scanline => {
                        let shift = if p.scanline_horizontal_only {
                            (y as f32 / hf * std::f32::consts::TAU * 4.0).sin() * p.radial_amount * wf as f32
                        } else {
                            0.0
                        };
                        (shift as i32, 0, 0, 0, -(shift as i32), 0)
                    }
                };

                let r_px = sample(&rgba, x as i32 + rdx, y as i32 + rdy);
                let g_px = sample(&rgba, x as i32 + gdx, y as i32 + gdy);
                let b_px = sample(&rgba, x as i32 + bdx, y as i32 + bdy);
                let orig = *rgba.get_pixel(x, y);

                out.put_pixel(x, y, Rgba([r_px[0], g_px[1], b_px[2], orig[3]]));
            }
        }

        DynamicImage::ImageRgba8(out)
    }

    fn params(&self) -> EffectParams { EffectParams::ChromaticAberration(self.0.clone()) }
    fn set_params(&mut self, params: EffectParams) {
        if let EffectParams::ChromaticAberration(p) = params { self.0 = p; }
    }
    fn clone_box(&self) -> Box<dyn Effect> { Box::new(ChromaticAberration(self.0.clone())) }
}
