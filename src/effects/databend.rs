use image::{DynamicImage, ImageBuffer, Rgba};
use serde::{Deserialize, Serialize};
use super::{Effect, EffectParams};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DatabendParams {
    pub intensity: f32,
    pub seed: u64,
    pub mode: DatabendMode,
    pub channel_mode: ChannelMode,
    pub block_w: u32,
    pub block_h: u32,
}

impl Default for DatabendParams {
    fn default() -> Self {
        Self {
            intensity: 0.05,
            seed: 42,
            mode: DatabendMode::RandomPixels,
            channel_mode: ChannelMode::FullPixel,
            block_w: 16,
            block_h: 16,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum DatabendMode {
    RandomPixels,
    GlitchBlocks,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ChannelMode {
    SingleChannel,
    FullPixel,
}

fn lcg(state: &mut u64) -> u64 {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    *state
}

fn flip_bits(v: u8, rng: &mut u64) -> u8 {
    let bit = (lcg(rng) % 8) as u8;
    v ^ (1 << bit)
}

pub struct Databend(pub DatabendParams);

impl Effect for Databend {
    fn name(&self) -> &str { "Databend" }

    fn apply(&self, img: &DynamicImage) -> DynamicImage {
        let p = &self.0;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        let mut pixels: Vec<Rgba<u8>> = rgba.pixels().copied().collect();
        let total = pixels.len();
        let mut rng = p.seed;

        match p.mode {
            DatabendMode::RandomPixels => {
                let affected = (total as f32 * p.intensity) as usize;
                for _ in 0..affected {
                    let idx = (lcg(&mut rng) as usize) % total;
                    let px = &mut pixels[idx];
                    match p.channel_mode {
                        ChannelMode::SingleChannel => {
                            let ch = (lcg(&mut rng) % 3) as usize;
                            px.0[ch] = flip_bits(px.0[ch], &mut rng);
                        }
                        ChannelMode::FullPixel => {
                            px.0[0] = flip_bits(px.0[0], &mut rng);
                            px.0[1] = flip_bits(px.0[1], &mut rng);
                            px.0[2] = flip_bits(px.0[2], &mut rng);
                        }
                    }
                }
            }
            DatabendMode::GlitchBlocks => {
                let bw = p.block_w.max(1);
                let bh = p.block_h.max(1);
                let num_blocks_x = (w + bw - 1) / bw;
                let num_blocks_y = (h + bh - 1) / bh;
                let total_blocks = (num_blocks_x * num_blocks_y) as usize;
                let affected_blocks = (total_blocks as f32 * p.intensity) as usize;
                for _ in 0..affected_blocks {
                    let bx = ((lcg(&mut rng) as u32) % num_blocks_x) * bw;
                    let by = ((lcg(&mut rng) as u32) % num_blocks_y) * bh;
                    let val_r = (lcg(&mut rng) % 256) as u8;
                    let val_g = (lcg(&mut rng) % 256) as u8;
                    let val_b = (lcg(&mut rng) % 256) as u8;
                    for dy in 0..bh {
                        for dx in 0..bw {
                            let px_x = bx + dx;
                            let py_y = by + dy;
                            if px_x < w && py_y < h {
                                let idx = (py_y * w + px_x) as usize;
                                let px = &mut pixels[idx];
                                match p.channel_mode {
                                    ChannelMode::SingleChannel => {
                                        let ch = (lcg(&mut rng) % 3) as usize;
                                        px.0[ch] = px.0[ch] ^ (val_r & 0b11110000);
                                    }
                                    ChannelMode::FullPixel => {
                                        px.0[0] ^= val_r;
                                        px.0[1] ^= val_g;
                                        px.0[2] ^= val_b;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut out = ImageBuffer::new(w, h);
        for (i, px) in pixels.iter().enumerate() {
            let x = (i as u32) % w;
            let y = (i as u32) / w;
            out.put_pixel(x, y, *px);
        }
        DynamicImage::ImageRgba8(out)
    }

    fn params(&self) -> EffectParams { EffectParams::Databend(self.0.clone()) }
    fn set_params(&mut self, params: EffectParams) {
        if let EffectParams::Databend(p) = params { self.0 = p; }
    }
    fn clone_box(&self) -> Box<dyn Effect> { Box::new(Databend(self.0.clone())) }
}
