use image::{DynamicImage, ImageBuffer, Rgba};
use serde::{Deserialize, Serialize};
use super::{Effect, EffectParams};
use super::dither::{Dither, DitherParams, DitherAlgorithm, DitherPalette};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuantizeParams {
    pub num_colors: usize,
    pub apply_dither: bool,
    pub dither_params: DitherParams,
}

impl Default for QuantizeParams {
    fn default() -> Self {
        Self {
            num_colors: 16,
            apply_dither: false,
            dither_params: DitherParams {
                algorithm: DitherAlgorithm::FloydSteinberg,
                palette: DitherPalette::Custom,
                scale_factor: 1,
                bayer_size: super::dither::BayerSize::B4,
                custom_colors: vec![],
                color_preserve: false,
            },
        }
    }
}

struct Bucket {
    pixels: Vec<[u8; 3]>,
}

impl Bucket {
    fn new(pixels: Vec<[u8; 3]>) -> Self { Self { pixels } }

    fn split(mut self) -> (Bucket, Bucket) {
        let (mut rmin, mut rmax) = (255u8, 0u8);
        let (mut gmin, mut gmax) = (255u8, 0u8);
        let (mut bmin, mut bmax) = (255u8, 0u8);
        for p in &self.pixels {
            rmin = rmin.min(p[0]); rmax = rmax.max(p[0]);
            gmin = gmin.min(p[1]); gmax = gmax.max(p[1]);
            bmin = bmin.min(p[2]); bmax = bmax.max(p[2]);
        }
        let rr = rmax - rmin;
        let gr = gmax - gmin;
        let br = bmax - bmin;
        if rr >= gr && rr >= br {
            self.pixels.sort_by_key(|p| p[0]);
        } else if gr >= rr && gr >= br {
            self.pixels.sort_by_key(|p| p[1]);
        } else {
            self.pixels.sort_by_key(|p| p[2]);
        }
        let mid = self.pixels.len() / 2;
        let b2 = self.pixels.split_off(mid);
        (Bucket::new(self.pixels), Bucket::new(b2))
    }

    fn average(&self) -> [u8; 3] {
        if self.pixels.is_empty() { return [0, 0, 0]; }
        let n = self.pixels.len() as u64;
        let r = self.pixels.iter().map(|p| p[0] as u64).sum::<u64>() / n;
        let g = self.pixels.iter().map(|p| p[1] as u64).sum::<u64>() / n;
        let b = self.pixels.iter().map(|p| p[2] as u64).sum::<u64>() / n;
        [r as u8, g as u8, b as u8]
    }
}

fn median_cut(pixels: Vec<[u8; 3]>, num_colors: usize) -> Vec<[u8; 3]> {
    let mut buckets = vec![Bucket::new(pixels)];
    while buckets.len() < num_colors {
        let largest = buckets.iter().enumerate().max_by_key(|(_, b)| b.pixels.len()).map(|(i, _)| i).unwrap_or(0);
        if buckets[largest].pixels.len() < 2 { break; }
        let bucket = buckets.remove(largest);
        let (a, b) = bucket.split();
        if !a.pixels.is_empty() { buckets.push(a); }
        if !b.pixels.is_empty() { buckets.push(b); }
    }
    buckets.iter().map(|b| b.average()).collect()
}

fn nearest(palette: &[[u8; 3]], r: u8, g: u8, b: u8) -> [u8; 3] {
    palette.iter().min_by_key(|c| {
        let dr = r as i32 - c[0] as i32;
        let dg = g as i32 - c[1] as i32;
        let db = b as i32 - c[2] as i32;
        dr*dr + dg*dg + db*db
    }).copied().unwrap_or([r, g, b])
}

pub struct Quantize(pub QuantizeParams);

impl Effect for Quantize {
    fn name(&self) -> &str { "Color Quantization" }

    fn apply(&self, img: &DynamicImage) -> DynamicImage {
        let p = &self.0;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        let pixels: Vec<[u8; 3]> = rgba.pixels().map(|px| [px[0], px[1], px[2]]).collect();
        let palette = median_cut(pixels, p.num_colors.max(2));

        if p.apply_dither {
            let mut dp = p.dither_params.clone();
            dp.palette = DitherPalette::Custom;
            dp.custom_colors = palette;
            let dither = Dither(dp);
            return dither.apply(img);
        }

        let mut out = ImageBuffer::new(w, h);
        for (i, px) in rgba.pixels().enumerate() {
            let x = (i as u32) % w;
            let y = (i as u32) / w;
            let c = nearest(&palette, px[0], px[1], px[2]);
            out.put_pixel(x, y, Rgba([c[0], c[1], c[2], px[3]]));
        }
        DynamicImage::ImageRgba8(out)
    }

    fn params(&self) -> EffectParams { EffectParams::Quantize(self.0.clone()) }
    fn set_params(&mut self, params: EffectParams) {
        if let EffectParams::Quantize(p) = params { self.0 = p; }
    }
    fn clone_box(&self) -> Box<dyn Effect> { Box::new(Quantize(self.0.clone())) }
}
