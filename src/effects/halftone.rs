use image::{DynamicImage, ImageBuffer, Rgba};
use serde::{Deserialize, Serialize};
use super::{Effect, EffectParams, luminance};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HalftoneParams {
    pub dot_size: f32,
    pub spacing: f32,
    pub angle: f32,
    pub mode: HalftoneMode,
    pub shape: HalftoneShape,
}

impl Default for HalftoneParams {
    fn default() -> Self {
        Self {
            dot_size: 10.0,
            spacing: 12.0,
            angle: 45.0,
            mode: HalftoneMode::Luminance,
            shape: HalftoneShape::Circle,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum HalftoneMode {
    Luminance,
    CMYK,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum HalftoneShape {
    Circle,
    Square,
    Line,
}

fn sample_bilinear(buf: &image::ImageBuffer<Rgba<u8>, Vec<u8>>, x: f32, y: f32) -> [f32; 4] {
    let (w, h) = buf.dimensions();
    let x = x.clamp(0.0, w as f32 - 1.0);
    let y = y.clamp(0.0, h as f32 - 1.0);
    let x0 = x as u32;
    let y0 = y as u32;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let p00 = buf.get_pixel(x0, y0);
    let p10 = buf.get_pixel(x1, y0);
    let p01 = buf.get_pixel(x0, y1);
    let p11 = buf.get_pixel(x1, y1);
    [0, 1, 2, 3].map(|c| {
        let v00 = p00[c] as f32;
        let v10 = p10[c] as f32;
        let v01 = p01[c] as f32;
        let v11 = p11[c] as f32;
        v00 * (1.0-fx)*(1.0-fy) + v10 * fx*(1.0-fy) + v01 * (1.0-fx)*fy + v11 * fx*fy
    })
}

fn draw_dot(out: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, cx: f32, cy: f32, radius: f32, color: [u8; 3], shape: &HalftoneShape) {
    let (w, h) = out.dimensions();
    let r = radius.ceil() as i32 + 1;
    let cx_i = cx as i32;
    let cy_i = cy as i32;
    for dy in -r..=r {
        for dx in -r..=r {
            let px = cx_i + dx;
            let py = cy_i + dy;
            if px < 0 || py < 0 || px >= w as i32 || py >= h as i32 { continue; }
            let inside = match shape {
                HalftoneShape::Circle => {
                    let fdx = px as f32 - cx;
                    let fdy = py as f32 - cy;
                    (fdx*fdx + fdy*fdy).sqrt() <= radius
                }
                HalftoneShape::Square => {
                    (px as f32 - cx).abs() <= radius && (py as f32 - cy).abs() <= radius
                }
                HalftoneShape::Line => {
                    (py as f32 - cy).abs() <= radius * 0.5
                }
            };
            if inside {
                out.put_pixel(px as u32, py as u32, Rgba([color[0], color[1], color[2], 255]));
            }
        }
    }
}

pub struct Halftone(pub HalftoneParams);

impl Effect for Halftone {
    fn name(&self) -> &str { "Halftone" }

    fn apply(&self, img: &DynamicImage) -> DynamicImage {
        let p = &self.0;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        let mut out: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_pixel(w, h, Rgba([255, 255, 255, 255]));

        let spacing = p.spacing.max(1.0);
        let angle_rad = p.angle.to_radians();
        let cos_a = angle_rad.cos();
        let sin_a = angle_rad.sin();
        let cx = w as f32 / 2.0;
        let cy = h as f32 / 2.0;

        let cols = ((w as f32 / spacing) as i32) + 4;
        let rows = ((h as f32 / spacing) as i32) + 4;
        let offset = ((cols.max(rows) as f32) * spacing / 2.0) as i32;

        match p.mode {
            HalftoneMode::Luminance => {
                for row in -offset/spacing as i32 ..= offset/spacing as i32 + rows {
                    for col in -offset/spacing as i32 ..= offset/spacing as i32 + cols {
                        let gx = col as f32 * spacing;
                        let gy = row as f32 * spacing;
                        let sx = cx + gx * cos_a - gy * sin_a;
                        let sy = cy + gx * sin_a + gy * cos_a;
                        if sx < 0.0 || sy < 0.0 || sx >= w as f32 || sy >= h as f32 { continue; }
                        let s = sample_bilinear(&rgba, sx, sy);
                        let lum = luminance(s[0] as u8, s[1] as u8, s[2] as u8);
                        let radius = (1.0 - lum) * p.dot_size * 0.5;
                        if radius > 0.3 {
                            draw_dot(&mut out, sx, sy, radius, [0, 0, 0], &p.shape);
                        }
                    }
                }
            }
            HalftoneMode::CMYK => {
                let channel_angles = [
                    (angle_rad + 15.0_f32.to_radians(), [0u8, 188, 188]),
                    (angle_rad + 75.0_f32.to_radians(), [255, 0, 144]),
                    (angle_rad + 90.0_f32.to_radians(), [255, 255, 0]),
                    (angle_rad, [0u8, 0, 0]),
                ];
                for (chan_angle, color) in channel_angles {
                    let ca = chan_angle.cos();
                    let sa = chan_angle.sin();
                    for row in -offset/spacing as i32 ..= offset/spacing as i32 + rows {
                        for col in -offset/spacing as i32 ..= offset/spacing as i32 + cols {
                            let gx = col as f32 * spacing;
                            let gy = row as f32 * spacing;
                            let sx = cx + gx * ca - gy * sa;
                            let sy = cy + gx * sa + gy * ca;
                            if sx < 0.0 || sy < 0.0 || sx >= w as f32 || sy >= h as f32 { continue; }
                            let s = sample_bilinear(&rgba, sx, sy);
                            let ch_val = match color {
                                [0, 188, 188] => 1.0 - s[0] / 255.0,
                                [255, 0, 144] => 1.0 - s[1] / 255.0,
                                [255, 255, 0] => 1.0 - s[2] / 255.0,
                                _ => luminance(s[0] as u8, s[1] as u8, s[2] as u8),
                            };
                            let radius = ch_val * p.dot_size * 0.5;
                            if radius > 0.3 {
                                draw_dot(&mut out, sx, sy, radius, color, &p.shape);
                            }
                        }
                    }
                }
            }
        }

        DynamicImage::ImageRgba8(out)
    }

    fn params(&self) -> EffectParams { EffectParams::Halftone(self.0.clone()) }
    fn set_params(&mut self, params: EffectParams) {
        if let EffectParams::Halftone(p) = params { self.0 = p; }
    }
    fn clone_box(&self) -> Box<dyn Effect> { Box::new(Halftone(self.0.clone())) }
}
