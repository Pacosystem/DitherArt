use super::{Effect, EffectParams};
use image::{DynamicImage, ImageBuffer};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PixelateParams {
    pub block_w: u32,
    pub block_h: u32,
    pub linked: bool,
}

impl Default for PixelateParams {
    fn default() -> Self {
        Self {
            block_w: 16,
            block_h: 16,
            linked: true,
        }
    }
}

pub struct Pixelate(pub PixelateParams);

impl Effect for Pixelate {
    fn name(&self) -> &str {
        "Pixelate"
    }

    fn apply(&self, img: &DynamicImage) -> DynamicImage {
        let p = &self.0;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        let bw = p.block_w.max(1);
        let bh = p.block_h.max(1);
        let blocks_x = (w + bw - 1) / bw;
        let blocks_y = (h + bh - 1) / bh;

        // Compute average color per block in parallel
        let block_colors: Vec<[u8; 4]> = (0..blocks_y * blocks_x)
            .into_par_iter()
            .map(|bi| {
                let bx = (bi % blocks_x) * bw;
                let by = (bi / blocks_x) * bh;
                let mut sum = [0u64; 4];
                let mut count = 0u64;
                for dy in 0..bh {
                    for dx in 0..bw {
                        let px = rgba.get_pixel((bx + dx).min(w - 1), (by + dy).min(h - 1));
                        for c in 0..4 {
                            sum[c] += px[c] as u64;
                        }
                        count += 1;
                    }
                }
                [0, 1, 2, 3].map(|c| (sum[c] / count) as u8)
            })
            .collect();

        // Fill output in parallel by row
        let out: Vec<u8> = (0..h)
            .into_par_iter()
            .flat_map(|y| {
                (0..w)
                    .map(|x| {
                        let bc = block_colors[((y / bh) * blocks_x + x / bw) as usize];
                        bc
                    })
                    .flat_map(|bc| bc)
                    .collect::<Vec<_>>()
            })
            .collect();

        let out = ImageBuffer::from_raw(w, h, out).unwrap();
        DynamicImage::ImageRgba8(out)
    }

    fn params(&self) -> EffectParams {
        EffectParams::Pixelate(self.0.clone())
    }
    fn set_params(&mut self, p: EffectParams) {
        if let EffectParams::Pixelate(v) = p {
            self.0 = v;
        }
    }
    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(Pixelate(self.0.clone()))
    }
}
