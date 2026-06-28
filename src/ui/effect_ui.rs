use crate::effects::bloom::BloomParams;
use crate::effects::chromatic::{ChromaticMode, ChromaticParams};
use crate::effects::color_adjust::ColorAdjustParams;
use crate::effects::databend::{ChannelMode, DatabendMode, DatabendParams};
use crate::effects::dither::{BayerSize, DitherAlgorithm, DitherPalette, DitherParams};
use crate::effects::halftone::{HalftoneMode, HalftoneParams, HalftoneShape};
use crate::effects::kuwahara::KuwaharaParams;
use crate::effects::noise::{NoiseMode, NoiseParams};
use crate::effects::pixel_sort::{PixelSortParams, SortBy, SortDirection};
use crate::effects::pixelate::PixelateParams;
use crate::effects::quantize::QuantizeParams;
use crate::effects::transform::{RotationMode, TransformParams};
use crate::effects::vhs::VhsParams;
use crate::effects::EffectParams;
use egui::Ui;

pub fn show_effect_params(ui: &mut Ui, params: &mut EffectParams) -> bool {
    match params {
        EffectParams::PixelSort(p) => show_pixel_sort(ui, p),
        EffectParams::Dither(p) => show_dither(ui, p),
        EffectParams::Quantize(p) => show_quantize(ui, p),
        EffectParams::Databend(p) => show_databend(ui, p),
        EffectParams::Halftone(p) => show_halftone(ui, p),
        EffectParams::ChromaticAberration(p) => show_chromatic(ui, p),
        EffectParams::ColorAdjust(p) => show_color_adjust(ui, p),
        EffectParams::Transform(p) => show_transform(ui, p),
        EffectParams::Pixelate(p) => show_pixelate(ui, p),
        EffectParams::Noise(p) => show_noise(ui, p),
        EffectParams::Bloom(p) => show_bloom(ui, p),
        EffectParams::Kuwahara(p) => show_kuwahara(ui, p),
        EffectParams::Vhs(p) => show_vhs(ui, p),
    }
}

/// Slider with double-click or right-click-reset to default.
fn rslider<T: egui::emath::Numeric>(
    ui: &mut Ui,
    label: &str,
    value: &mut T,
    range: std::ops::RangeInclusive<T>,
    default: T,
    configure: impl FnOnce(egui::Slider<'_>) -> egui::Slider<'_>,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        let resp = ui.add(configure(egui::Slider::new(value, range)));

        // Right-click: reset menu
        resp.context_menu(|ui| {
            if ui.button("Reset to default").clicked() {
                *value = default;
                changed = true;
                ui.close_menu();
            }
        });

        // Double-click: manual detection via egui memory
        let dc_id = resp.id.with("dc");
        if resp.clicked() {
            let now = ui.input(|i| i.time);
            let last: f64 = ui.memory(|m| m.data.get_temp(dc_id).unwrap_or(0.0));
            if now - last < 0.45 && last > 0.0 {
                *value = default;
                changed = true;
                ui.memory_mut(|m| m.data.remove::<f64>(dc_id));
                return; // skip changed check below
            }
            ui.memory_mut(|m| m.data.insert_temp(dc_id, now));
        }

        if resp.changed() {
            changed = true;
        }
    });
    changed
}

fn show_pixel_sort(ui: &mut Ui, p: &mut PixelSortParams) -> bool {
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.label("Direction:");
        changed |= ui
            .selectable_value(&mut p.direction, SortDirection::Horizontal, "Horizontal")
            .changed();
        changed |= ui
            .selectable_value(&mut p.direction, SortDirection::Vertical, "Vertical")
            .changed();
        changed |= ui
            .selectable_value(&mut p.direction, SortDirection::Diagonal, "Diagonal")
            .changed();
    });

    ui.horizontal(|ui| {
        ui.label("Sort by:");
        changed |= ui
            .selectable_value(&mut p.sort_by, SortBy::Luminance, "Luma")
            .changed();
        changed |= ui
            .selectable_value(&mut p.sort_by, SortBy::Hue, "Hue")
            .changed();
        changed |= ui
            .selectable_value(&mut p.sort_by, SortBy::Saturation, "Sat")
            .changed();
        changed |= ui
            .selectable_value(&mut p.sort_by, SortBy::Red, "R")
            .changed();
        changed |= ui
            .selectable_value(&mut p.sort_by, SortBy::Green, "G")
            .changed();
        changed |= ui
            .selectable_value(&mut p.sort_by, SortBy::Blue, "B")
            .changed();
    });

    ui.horizontal(|ui| {
        ui.label("Order:");
        if ui
            .button(if p.ascending {
                "Ascending"
            } else {
                "Descending"
            })
            .clicked()
        {
            p.ascending = !p.ascending;
            changed = true;
        }
    });

    if ui.checkbox(&mut p.use_segments, "Segment mode").changed() {
        changed = true;
    }

    if p.use_segments {
        let mut s = p.segment_size as f32;
        changed |= rslider(ui, "Segment size:", &mut s, 2.0..=1000.0, 100.0, |sl| {
            sl.integer()
        });
        p.segment_size = s as usize;
    } else {
        changed |= rslider(
            ui,
            "Threshold low:",
            &mut p.threshold_low,
            0.0..=1.0,
            0.1,
            |s| s,
        );
        changed |= rslider(
            ui,
            "Threshold high:",
            &mut p.threshold_high,
            0.0..=1.0,
            0.9,
            |s| s,
        );
    }

    changed
}

fn show_dither(ui: &mut Ui, p: &mut DitherParams) -> bool {
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.label("Algorithm:");
        changed |= ui
            .selectable_value(
                &mut p.algorithm,
                DitherAlgorithm::FloydSteinberg,
                "Floyd-Steinberg",
            )
            .changed();
        changed |= ui
            .selectable_value(&mut p.algorithm, DitherAlgorithm::Atkinson, "Atkinson")
            .changed();
        changed |= ui
            .selectable_value(&mut p.algorithm, DitherAlgorithm::Ordered, "Ordered")
            .changed();
        changed |= ui
            .selectable_value(&mut p.algorithm, DitherAlgorithm::Random, "Random")
            .changed();
    });

    if p.algorithm == DitherAlgorithm::Ordered {
        ui.horizontal(|ui| {
            ui.label("Bayer size:");
            changed |= ui
                .selectable_value(&mut p.bayer_size, BayerSize::B2, "2x2")
                .changed();
            changed |= ui
                .selectable_value(&mut p.bayer_size, BayerSize::B4, "4x4")
                .changed();
            changed |= ui
                .selectable_value(&mut p.bayer_size, BayerSize::B8, "8x8")
                .changed();
        });
    }

    ui.horizontal(|ui| {
        ui.label("Palette:");
        changed |= ui
            .selectable_value(&mut p.palette, DitherPalette::BW, "B&W")
            .changed();
        changed |= ui
            .selectable_value(&mut p.palette, DitherPalette::Color4, "4-color")
            .changed();
        changed |= ui
            .selectable_value(&mut p.palette, DitherPalette::Color8, "8-color")
            .changed();
        changed |= ui
            .selectable_value(&mut p.palette, DitherPalette::Color16, "16-color")
            .changed();
        changed |= ui
            .selectable_value(&mut p.palette, DitherPalette::Custom, "Custom")
            .changed();
    });

    if p.palette == DitherPalette::Custom {
        ui.label("Custom colors:");
        let mut to_remove = None;
        for (i, color) in p.custom_colors.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                let mut c32 = egui::Color32::from_rgb(color[0], color[1], color[2]);
                if ui.color_edit_button_srgba(&mut c32).changed() {
                    color[0] = c32.r();
                    color[1] = c32.g();
                    color[2] = c32.b();
                    changed = true;
                }
                if ui.small_button("x").clicked() {
                    to_remove = Some(i);
                }
            });
        }
        if let Some(i) = to_remove {
            p.custom_colors.remove(i);
            changed = true;
        }
        if ui.small_button("+ color").clicked() {
            p.custom_colors.push([128, 128, 128]);
            changed = true;
        }
    }

    let mut s = p.scale_factor as f32;
    if rslider(ui, "Scale factor:", &mut s, 1.0..=16.0, 1.0, |sl| {
        sl.integer()
    }) {
        p.scale_factor = s as u32;
        changed = true;
    }

    ui.separator();
    if ui
        .checkbox(&mut p.color_preserve, "Preserve original colors")
        .on_hover_text(
            "B&W dither as mask: white areas show original color, black areas stay black",
        )
        .changed()
    {
        changed = true;
    }

    changed
}

fn show_quantize(ui: &mut Ui, p: &mut QuantizeParams) -> bool {
    let mut changed = false;

    let mut n = p.num_colors as f32;
    if rslider(ui, "Colors:", &mut n, 2.0..=256.0, 16.0, |sl| sl.integer()) {
        p.num_colors = n as usize;
        changed = true;
    }

    if ui
        .checkbox(&mut p.apply_dither, "Apply dithering after")
        .changed()
    {
        changed = true;
    }

    if p.apply_dither {
        ui.indent("dither_sub", |ui| {
            changed |= show_dither(ui, &mut p.dither_params);
        });
    }

    changed
}

fn show_databend(ui: &mut Ui, p: &mut DatabendParams) -> bool {
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.label("Mode:");
        changed |= ui
            .selectable_value(&mut p.mode, DatabendMode::RandomPixels, "Random Pixels")
            .changed();
        changed |= ui
            .selectable_value(&mut p.mode, DatabendMode::GlitchBlocks, "Glitch Blocks")
            .changed();
    });

    ui.horizontal(|ui| {
        ui.label("Channel:");
        changed |= ui
            .selectable_value(&mut p.channel_mode, ChannelMode::SingleChannel, "Single")
            .changed();
        changed |= ui
            .selectable_value(&mut p.channel_mode, ChannelMode::FullPixel, "Full Pixel")
            .changed();
    });

    changed |= rslider(ui, "Intensity:", &mut p.intensity, 0.0..=1.0, 0.05, |s| s);

    ui.horizontal(|ui| {
        ui.label("Seed:");
        if ui.add(egui::DragValue::new(&mut p.seed)).changed() {
            changed = true;
        }
    });

    if p.mode == DatabendMode::GlitchBlocks {
        let mut bw = p.block_w as f32;
        if rslider(ui, "Block W:", &mut bw, 1.0..=256.0, 16.0, |sl| {
            sl.integer()
        }) {
            p.block_w = bw as u32;
            changed = true;
        }
        let mut bh = p.block_h as f32;
        if rslider(ui, "Block H:", &mut bh, 1.0..=256.0, 16.0, |sl| {
            sl.integer()
        }) {
            p.block_h = bh as u32;
            changed = true;
        }
    }

    changed
}

fn show_halftone(ui: &mut Ui, p: &mut HalftoneParams) -> bool {
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.label("Mode:");
        changed |= ui
            .selectable_value(&mut p.mode, HalftoneMode::Luminance, "Luminance")
            .changed();
        changed |= ui
            .selectable_value(&mut p.mode, HalftoneMode::CMYK, "CMYK")
            .changed();
    });

    ui.horizontal(|ui| {
        ui.label("Shape:");
        changed |= ui
            .selectable_value(&mut p.shape, HalftoneShape::Circle, "Circle")
            .changed();
        changed |= ui
            .selectable_value(&mut p.shape, HalftoneShape::Square, "Square")
            .changed();
        changed |= ui
            .selectable_value(&mut p.shape, HalftoneShape::Line, "Line")
            .changed();
    });

    changed |= rslider(ui, "Dot size:", &mut p.dot_size, 1.0..=50.0, 10.0, |s| s);
    changed |= rslider(ui, "Spacing:", &mut p.spacing, 1.0..=60.0, 12.0, |s| s);
    changed |= rslider(ui, "Angle:", &mut p.angle, 0.0..=90.0, 45.0, |s| s);

    changed
}

fn show_chromatic(ui: &mut Ui, p: &mut ChromaticParams) -> bool {
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.label("Mode:");
        changed |= ui
            .selectable_value(&mut p.mode, ChromaticMode::Fixed, "Fixed")
            .changed();
        changed |= ui
            .selectable_value(&mut p.mode, ChromaticMode::Radial, "Radial")
            .changed();
        changed |= ui
            .selectable_value(&mut p.mode, ChromaticMode::Scanline, "Scanline")
            .changed();
    });

    match p.mode {
        ChromaticMode::Fixed => {
            changed |= rslider(ui, "R shift X:", &mut p.r_dx, -100..=100, -5, |s| s);
            changed |= rslider(ui, "R shift Y:", &mut p.r_dy, -100..=100, 0, |s| s);
            changed |= rslider(ui, "G shift X:", &mut p.g_dx, -100..=100, 0, |s| s);
            changed |= rslider(ui, "G shift Y:", &mut p.g_dy, -100..=100, 0, |s| s);
            changed |= rslider(ui, "B shift X:", &mut p.b_dx, -100..=100, 5, |s| s);
            changed |= rslider(ui, "B shift Y:", &mut p.b_dy, -100..=100, 0, |s| s);
        }
        ChromaticMode::Radial => {
            changed |= rslider(ui, "Amount:", &mut p.radial_amount, 0.0..=0.5, 0.05, |s| s);
            changed |= rslider(ui, "Angle:", &mut p.angle, 0.0..=360.0, 0.0, |s| s);
        }
        ChromaticMode::Scanline => {
            changed |= rslider(
                ui,
                "Strength:",
                &mut p.radial_amount,
                0.0..=0.1,
                0.02,
                |s| s,
            );
            if ui
                .checkbox(&mut p.scanline_horizontal_only, "Horizontal only")
                .changed()
            {
                changed = true;
            }
        }
    }

    changed
}

fn show_color_adjust(ui: &mut Ui, p: &mut ColorAdjustParams) -> bool {
    let mut changed = false;

    ui.horizontal(|ui| {
        if ui.checkbox(&mut p.grayscale, "Grayscale").changed() {
            changed = true;
        }
        if ui.checkbox(&mut p.sepia, "Sepia").changed() {
            changed = true;
        }
        if ui.checkbox(&mut p.invert, "Invert").changed() {
            changed = true;
        }
    });

    ui.separator();

    changed |= rslider(ui, "Brightness:", &mut p.brightness, -1.0..=1.0, 0.0, |s| s);
    changed |= rslider(ui, "Contrast:", &mut p.contrast, -1.0..=1.0, 0.0, |s| s);
    changed |= rslider(ui, "Saturation:", &mut p.saturation, -1.0..=1.0, 0.0, |s| s);
    changed |= rslider(
        ui,
        "Hue shift:",
        &mut p.hue_shift,
        -180.0..=180.0,
        0.0,
        |s| s.suffix("°"),
    );
    changed |= rslider(
        ui,
        "Temperature:",
        &mut p.temperature,
        -1.0..=1.0,
        0.0,
        |s| {
            s.custom_formatter(|v, _| {
                if v < 0.0 {
                    format!("Cool {:.2}", v)
                } else {
                    format!("Warm {:.2}", v)
                }
            })
        },
    );

    changed
}

fn show_transform(ui: &mut Ui, p: &mut TransformParams) -> bool {
    let mut changed = false;

    // ── Rotation ──────────────────────────────────────────────────────────────
    ui.label("Rotation:");
    ui.horizontal_wrapped(|ui| {
        changed |= ui
            .selectable_value(&mut p.rotation, RotationMode::None, "None")
            .changed();
        changed |= ui
            .selectable_value(&mut p.rotation, RotationMode::Rotate90, "90°")
            .changed();
        changed |= ui
            .selectable_value(&mut p.rotation, RotationMode::Rotate180, "180°")
            .changed();
        changed |= ui
            .selectable_value(&mut p.rotation, RotationMode::Rotate270, "270°")
            .changed();
        changed |= ui
            .selectable_value(&mut p.rotation, RotationMode::Arbitrary, "Custom")
            .changed();
    });

    if p.rotation == RotationMode::Arbitrary {
        changed |= rslider(ui, "Angle:", &mut p.angle, -180.0..=180.0, 0.0, |s| {
            s.suffix("°")
        });
        if ui
            .checkbox(&mut p.expand_canvas, "Expand canvas to fit")
            .changed()
        {
            changed = true;
        }
    }

    // ── Flip ──────────────────────────────────────────────────────────────────
    ui.separator();
    ui.horizontal(|ui| {
        if ui.checkbox(&mut p.flip_h, "Flip Horizontal").changed() {
            changed = true;
        }
        if ui.checkbox(&mut p.flip_v, "Flip Vertical").changed() {
            changed = true;
        }
    });

    // ── Crop ──────────────────────────────────────────────────────────────────
    ui.separator();
    if ui.checkbox(&mut p.crop_enabled, "Crop").changed() {
        changed = true;
    }

    if p.crop_enabled {
        changed |= rslider(ui, "Left (X):", &mut p.crop_x, 0.0..=99.0, 0.0, |s| {
            s.suffix("%")
        });
        changed |= rslider(ui, "Top (Y):", &mut p.crop_y, 0.0..=99.0, 0.0, |s| {
            s.suffix("%")
        });
        changed |= rslider(ui, "Width:", &mut p.crop_w, 1.0..=100.0, 100.0, |s| {
            s.suffix("%")
        });
        changed |= rslider(ui, "Height:", &mut p.crop_h, 1.0..=100.0, 100.0, |s| {
            s.suffix("%")
        });

        // Visual hint about resulting crop
        let effective_w = (p.crop_w).min(100.0 - p.crop_x);
        let effective_h = (p.crop_h).min(100.0 - p.crop_y);
        ui.label(
            egui::RichText::new(format!(
                "  Crops to {:.0}% × {:.0}% of image",
                effective_w, effective_h
            ))
            .weak()
            .small(),
        );
    }

    changed
}

fn show_pixelate(ui: &mut Ui, p: &mut PixelateParams) -> bool {
    let mut changed = false;
    let mut bw = p.block_w as f32;
    if rslider(ui, "Block W:", &mut bw, 2.0..=100.0, 16.0, |s| s.integer()) {
        p.block_w = bw as u32;
        if p.linked {
            p.block_h = bw as u32;
        }
        changed = true;
    }
    if !p.linked {
        let mut bh = p.block_h as f32;
        if rslider(ui, "Block H:", &mut bh, 2.0..=100.0, 16.0, |s| s.integer()) {
            p.block_h = bh as u32;
            changed = true;
        }
    }
    if ui.checkbox(&mut p.linked, "Link W/H").changed() {
        if p.linked {
            p.block_h = p.block_w;
        }
        changed = true;
    }
    changed
}

fn show_noise(ui: &mut Ui, p: &mut NoiseParams) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("Mode:");
        changed |= ui
            .selectable_value(&mut p.mode, NoiseMode::Gaussian, "Gaussian")
            .changed();
        changed |= ui
            .selectable_value(&mut p.mode, NoiseMode::Uniform, "Uniform")
            .changed();
    });
    changed |= rslider(ui, "Intensity:", &mut p.intensity, 0.0..=1.0, 0.1, |s| s);
    ui.horizontal(|ui| {
        ui.label("Seed:");
        if ui.add(egui::DragValue::new(&mut p.seed)).changed() {
            changed = true;
        }
    });
    if ui.checkbox(&mut p.monochrome, "Monochrome").changed() {
        changed = true;
    }
    changed
}

fn show_bloom(ui: &mut Ui, p: &mut BloomParams) -> bool {
    let mut changed = false;
    changed |= rslider(ui, "Threshold:", &mut p.threshold, 0.0..=1.0, 0.7, |s| s);
    changed |= rslider(ui, "Intensity:", &mut p.intensity, 0.0..=3.0, 0.8, |s| s);
    changed |= rslider(ui, "Radius:", &mut p.radius, 0.5..=50.0, 15.0, |s| s);
    changed
}

fn show_kuwahara(ui: &mut Ui, p: &mut KuwaharaParams) -> bool {
    let mut changed = false;
    let mut r = p.radius as f32;
    if rslider(ui, "Radius:", &mut r, 1.0..=20.0, 6.0, |s| s.integer()) {
        p.radius = r as i32;
        changed = true;
    }
    ui.label(
        egui::RichText::new("Edge-preserving painterly filter")
            .weak()
            .small(),
    );
    changed
}

fn show_vhs(ui: &mut Ui, p: &mut VhsParams) -> bool {
    let mut changed = false;
    ui.label(egui::RichText::new("CRT / VHS tape simulation").weak());
    ui.separator();
    changed |= rslider(
        ui,
        "Scanline intensity:",
        &mut p.scanline_intensity,
        0.0..=1.0,
        0.25,
        |s| s,
    );
    {
        let mut g = p.scanline_gap as f32;
        if rslider(ui, "Scanline gap:", &mut g, 1.0..=4.0, 2.0, |s| s.integer()) {
            p.scanline_gap = g as u32;
            changed = true;
        }
    }
    ui.separator();
    changed |= rslider(
        ui,
        "Color bleed:",
        &mut p.color_bleed,
        0.0..=20.0,
        4.0,
        |s| s,
    );
    changed |= rslider(ui, "Noise:", &mut p.noise, 0.0..=0.5, 0.04, |s| s);
    ui.horizontal(|ui| {
        ui.label("Noise seed:");
        if ui.add(egui::DragValue::new(&mut p.noise_seed)).changed() {
            changed = true;
        }
    });
    ui.separator();
    changed |= rslider(ui, "Vignette:", &mut p.vignette, 0.0..=1.0, 0.4, |s| s);
    changed |= rslider(
        ui,
        "Barrel distortion:",
        &mut p.barrel,
        0.0..=0.3,
        0.05,
        |s| s,
    );
    changed |= rslider(
        ui,
        "Tracking distortion:",
        &mut p.tracking_distortion,
        0.0..=1.0,
        0.0,
        |s| s,
    );
    changed
}
