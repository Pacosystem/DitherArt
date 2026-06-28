use image::{DynamicImage, ImageBuffer, Rgba};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use super::{Effect, EffectParams, luminance, rgb_to_hsl};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PixelSortParams {
    pub direction: SortDirection,
    pub sort_by: SortBy,
    pub ascending: bool,
    pub threshold_low: f32,
    pub threshold_high: f32,
    pub segment_size: usize,
    pub use_segments: bool,
}

impl Default for PixelSortParams {
    fn default() -> Self {
        Self {
            direction: SortDirection::Horizontal,
            sort_by: SortBy::Luminance,
            ascending: true,
            threshold_low: 0.1,
            threshold_high: 0.9,
            segment_size: 100,
            use_segments: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum SortDirection {
    Horizontal,
    Vertical,
    Diagonal,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum SortBy {
    Luminance,
    Hue,
    Saturation,
    Red,
    Green,
    Blue,
}

fn sort_key(p: &Rgba<u8>, sort_by: &SortBy) -> f32 {
    let [r, g, b, _] = p.0;
    match sort_by {
        SortBy::Luminance => luminance(r, g, b),
        SortBy::Hue => rgb_to_hsl(r, g, b).0,
        SortBy::Saturation => rgb_to_hsl(r, g, b).1,
        SortBy::Red => r as f32 / 255.0,
        SortBy::Green => g as f32 / 255.0,
        SortBy::Blue => b as f32 / 255.0,
    }
}

fn sort_segment(pixels: &mut [Rgba<u8>], sort_by: &SortBy, ascending: bool, low: f32, high: f32) {
    let mut i = 0;
    while i < pixels.len() {
        let key = sort_key(&pixels[i], sort_by);
        if key >= low && key <= high {
            let start = i;
            while i < pixels.len() {
                let k = sort_key(&pixels[i], sort_by);
                if k < low || k > high {
                    break;
                }
                i += 1;
            }
            let seg = &mut pixels[start..i];
            if ascending {
                seg.sort_by(|a, b| sort_key(a, sort_by).partial_cmp(&sort_key(b, sort_by)).unwrap());
            } else {
                seg.sort_by(|a, b| sort_key(b, sort_by).partial_cmp(&sort_key(a, sort_by)).unwrap());
            }
        } else {
            i += 1;
        }
    }
}

fn sort_chunk(pixels: &mut [Rgba<u8>], sort_by: &SortBy, ascending: bool, chunk: usize) {
    for seg in pixels.chunks_mut(chunk) {
        if ascending {
            seg.sort_by(|a, b| sort_key(a, sort_by).partial_cmp(&sort_key(b, sort_by)).unwrap());
        } else {
            seg.sort_by(|a, b| sort_key(b, sort_by).partial_cmp(&sort_key(a, sort_by)).unwrap());
        }
    }
}

pub struct PixelSort(pub PixelSortParams);

impl Effect for PixelSort {
    fn name(&self) -> &str { "Pixel Sort" }

    fn apply(&self, img: &DynamicImage) -> DynamicImage {
        let p = &self.0;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();

        match p.direction {
            SortDirection::Horizontal => {
                let mut rows: Vec<Vec<Rgba<u8>>> = (0..h)
                    .map(|y| (0..w).map(|x| *rgba.get_pixel(x, y)).collect())
                    .collect();
                rows.par_iter_mut().for_each(|row| {
                    if p.use_segments {
                        sort_chunk(row, &p.sort_by, p.ascending, p.segment_size.max(1));
                    } else {
                        sort_segment(row, &p.sort_by, p.ascending, p.threshold_low, p.threshold_high);
                    }
                });
                let mut out = ImageBuffer::new(w, h);
                for (y, row) in rows.iter().enumerate() {
                    for (x, px) in row.iter().enumerate() {
                        out.put_pixel(x as u32, y as u32, *px);
                    }
                }
                DynamicImage::ImageRgba8(out)
            }
            SortDirection::Vertical => {
                let mut cols: Vec<Vec<Rgba<u8>>> = (0..w)
                    .map(|x| (0..h).map(|y| *rgba.get_pixel(x, y)).collect())
                    .collect();
                cols.par_iter_mut().for_each(|col| {
                    if p.use_segments {
                        sort_chunk(col, &p.sort_by, p.ascending, p.segment_size.max(1));
                    } else {
                        sort_segment(col, &p.sort_by, p.ascending, p.threshold_low, p.threshold_high);
                    }
                });
                let mut out = ImageBuffer::new(w, h);
                for (x, col) in cols.iter().enumerate() {
                    for (y, px) in col.iter().enumerate() {
                        out.put_pixel(x as u32, y as u32, *px);
                    }
                }
                DynamicImage::ImageRgba8(out)
            }
            SortDirection::Diagonal => {
                let total = (w + h) as usize - 1;
                let mut diags: Vec<Vec<(u32, u32, Rgba<u8>)>> = (0..total)
                    .map(|d| {
                        let mut diag = vec![];
                        let d = d as u32;
                        for x in 0..w {
                            if d >= x && d - x < h {
                                let y = d - x;
                                diag.push((x, y, *rgba.get_pixel(x, y)));
                            }
                        }
                        diag
                    })
                    .collect();
                diags.par_iter_mut().for_each(|diag| {
                    let mut pixels: Vec<Rgba<u8>> = diag.iter().map(|(_, _, px)| *px).collect();
                    if p.use_segments {
                        sort_chunk(&mut pixels, &p.sort_by, p.ascending, p.segment_size.max(1));
                    } else {
                        sort_segment(&mut pixels, &p.sort_by, p.ascending, p.threshold_low, p.threshold_high);
                    }
                    for (i, (_, _, px)) in diag.iter_mut().enumerate() {
                        *px = pixels[i];
                    }
                });
                let mut out = ImageBuffer::new(w, h);
                for diag in &diags {
                    for (x, y, px) in diag {
                        out.put_pixel(*x, *y, *px);
                    }
                }
                DynamicImage::ImageRgba8(out)
            }
        }
    }

    fn params(&self) -> EffectParams { EffectParams::PixelSort(self.0.clone()) }
    fn set_params(&mut self, params: EffectParams) {
        if let EffectParams::PixelSort(p) = params { self.0 = p; }
    }
    fn clone_box(&self) -> Box<dyn Effect> { Box::new(PixelSort(self.0.clone())) }
}
