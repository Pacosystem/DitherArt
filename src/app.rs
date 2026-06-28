#[allow(unused_imports)]
use image::GenericImageView as _;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use eframe::egui;
use egui::{ColorImage, Context, Pos2, Rect, TextureHandle, Vec2};
use image::DynamicImage;

use crate::anim::{AnimParam, StepAnims};
use crate::effects::EffectParams;
use crate::gpu::GpuCompute;
use crate::pipeline::{Pipeline, PipelineStep, Preset};
use crate::ui::effect_ui::show_effect_params;

#[derive(Clone, PartialEq)]
pub enum CompareMode {
    Single,
    SideBySide,
}

struct RenderResult {
    image: DynamicImage,
    render_ms: u64,
}

/// Results from file dialog threads
enum FileMsg {
    OpenImage(PathBuf),
    ExportImage(PathBuf),
    ExportGif(PathBuf),
    SavePreset(PathBuf),
    LoadPreset(PathBuf),
}

pub struct App {
    source_image: Option<DynamicImage>,
    result_texture: Option<TextureHandle>,
    source_texture: Option<TextureHandle>,

    pub pipeline: Vec<PipelineStep>,
    pub anim_states: Vec<StepAnims>,

    source_arc: Arc<Mutex<Option<DynamicImage>>>,
    pipeline_arc: Arc<Mutex<Vec<PipelineStep>>>,

    gpu: Option<Arc<GpuCompute>>,

    render_tx: Sender<()>,
    result_rx: Receiver<RenderResult>,

    // File dialog results arrive here from background threads
    file_tx: Sender<FileMsg>,
    file_rx: Receiver<FileMsg>,

    last_change: Option<Instant>,
    is_rendering: bool,

    pub compare_mode: CompareMode,
    pub zoom: f32,
    pub pan: Vec2,

    pub status_dims: Option<(u32, u32)>,
    pub status_render_ms: u64,
    pub status_msg: Option<String>,

    pub undo_stack: Vec<Vec<PipelineStep>>,
    pub redo_stack: Vec<Vec<PipelineStep>>,

    pub show_add_effect: bool,
    pub randomize_seed: u64,

    move_highlight: Option<(usize, Instant)>,

    /// RGB + Luminance histograms (256 bins each)
    histogram: Option<([u32; 256], [u32; 256], [u32; 256], [u32; 256])>,
}

fn dynamic_to_color_image(img: &DynamicImage) -> ColorImage {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let pixels: Vec<egui::Color32> = rgba
        .pixels()
        .map(|p| egui::Color32::from_rgba_premultiplied(p[0], p[1], p[2], p[3]))
        .collect();
    ColorImage {
        size: [w as usize, h as usize],
        pixels,
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (render_tx, render_rx_inner) = mpsc::channel::<()>();
        let (result_tx, result_rx) = mpsc::channel::<RenderResult>();

        let gpu: Option<Arc<GpuCompute>> = cc
            .wgpu_render_state
            .as_ref()
            .map(|rs| Arc::new(GpuCompute::new(rs.device.clone(), rs.queue.clone())));

        let source_arc: Arc<Mutex<Option<DynamicImage>>> = Arc::new(Mutex::new(None));
        let pipeline_arc: Arc<Mutex<Vec<PipelineStep>>> = Arc::new(Mutex::new(vec![]));

        let source_clone = source_arc.clone();
        let pipeline_clone = pipeline_arc.clone();
        let gpu_thread = gpu.clone();

        std::thread::spawn(move || {
            while render_rx_inner.recv().is_ok() {
                let src = source_clone.lock().unwrap().clone();
                let steps = pipeline_clone.lock().unwrap().clone();
                if let Some(img) = src {
                    let start = Instant::now();
                    let pipeline = Pipeline::from_steps(&steps);
                    let result = pipeline.run(&img, gpu_thread.as_deref());
                    let ms = start.elapsed().as_millis() as u64;
                    let _ = result_tx.send(RenderResult {
                        image: result,
                        render_ms: ms,
                    });
                }
            }
        });

        let (file_tx, file_rx) = mpsc::channel::<FileMsg>();

        App {
            source_image: None,
            result_texture: None,
            source_texture: None,
            pipeline: vec![],
            anim_states: vec![],
            gpu,
            source_arc,
            pipeline_arc,
            render_tx,
            result_rx,
            file_tx,
            file_rx,
            last_change: None,
            is_rendering: false,
            compare_mode: CompareMode::Single,
            zoom: 1.0,
            pan: Vec2::ZERO,
            status_dims: None,
            status_msg: None,
            status_render_ms: 0,
            show_add_effect: false,
            randomize_seed: 0,
            move_highlight: None,
            histogram: None,
            undo_stack: vec![],
            redo_stack: vec![],
        }
    }

    pub fn load_image(&mut self, path: PathBuf) {
        if let Ok(img) = image::open(&path) {
            let (w, h) = img.dimensions();
            self.status_dims = Some((w, h));
            *self.source_arc.lock().unwrap() = Some(img.clone());
            self.source_image = Some(img);
            self.source_texture = None; // force rebuild
            self.trigger_render();
        }
    }

    pub fn load_image_from_bytes(&mut self, bytes: &[u8]) {
        if let Ok(img) = image::load_from_memory(bytes) {
            let (w, h) = img.dimensions();
            self.status_dims = Some((w, h));
            *self.source_arc.lock().unwrap() = Some(img.clone());
            self.source_image = Some(img);
            self.source_texture = None;
            self.trigger_render();
        }
    }

    pub fn has_animation(&self) -> bool {
        self.anim_states
            .iter()
            .any(|m| m.values().any(|a| a.enabled))
    }

    pub fn push_undo(&mut self) {
        self.undo_stack.push(self.pipeline.clone());
        if self.undo_stack.len() > 20 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    /// Ensure anim_states length matches pipeline length
    fn sync_anim_states(&mut self) {
        while self.anim_states.len() < self.pipeline.len() {
            self.anim_states.push(HashMap::new());
        }
        self.anim_states.truncate(self.pipeline.len());
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(self.pipeline.clone());
            self.pipeline = prev;
            self.trigger_render();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(self.pipeline.clone());
            self.pipeline = next;
            self.trigger_render();
        }
    }

    pub fn trigger_render(&mut self) {
        self.last_change = Some(Instant::now());
    }

    fn maybe_dispatch_render(&mut self) {
        // Dispatch immediately when idle — no debounce needed since render runs on a background thread.
        // If already rendering, changes accumulate in last_change and fire on the next poll.
        if self.last_change.is_some() && !self.is_rendering {
            self.last_change = None;
            self.is_rendering = true;
            *self.pipeline_arc.lock().unwrap() = self.pipeline.clone();
            let _ = self.render_tx.send(());
        }
    }

    fn poll_result(&mut self, ctx: &Context) {
        if let Ok(result) = self.result_rx.try_recv() {
            self.status_render_ms = result.render_ms;

            // Compute histogram from result
            let rgba = result.image.to_rgba8();
            let mut hr = [0u32; 256];
            let mut hg = [0u32; 256];
            let mut hb = [0u32; 256];
            let mut hl = [0u32; 256];
            for px in rgba.pixels() {
                hr[px[0] as usize] += 1;
                hg[px[1] as usize] += 1;
                hb[px[2] as usize] += 1;
                let l =
                    (0.2126 * px[0] as f32 + 0.7152 * px[1] as f32 + 0.0722 * px[2] as f32) as u8;
                hl[l as usize] += 1;
            }
            self.histogram = Some((hr, hg, hb, hl));

            let ci = dynamic_to_color_image(&result.image);
            self.result_texture =
                Some(ctx.load_texture("result", ci, egui::TextureOptions::LINEAR));
            self.is_rendering = false;
        }
    }

    fn update_source_texture(&mut self, ctx: &Context) {
        if self.source_texture.is_none() {
            if let Some(src) = &self.source_image {
                let ci = dynamic_to_color_image(src);
                self.source_texture =
                    Some(ctx.load_texture("source", ci, egui::TextureOptions::LINEAR));
            }
        }
    }

    pub fn export_image(&mut self, path: PathBuf) {
        if let Some(src) = &self.source_image {
            let pipeline = Pipeline::from_steps(&self.pipeline);
            let result = pipeline.run(src, self.gpu.as_deref());
            match result.save(&path) {
                Ok(_) => self.status_msg = Some(format!("Saved: {}", path.display())),
                Err(e) => self.status_msg = Some(format!("Save error: {e}")),
            }
        }
    }

    pub fn export_gif(&mut self, path: PathBuf) {
        let Some(src) = self.source_image.clone() else {
            return;
        };

        let fps = 12u32;
        let duration = 2.0f64;
        let total_frames = (fps as f64 * duration) as u32;
        // gif delay is in centiseconds (1/100 s)
        let delay_cs = (100 / fps) as u16;

        // Downscale: GIF is limited, 600px is plenty for preview quality
        let max_dim = 600u32;
        let (sw, sh) = src.dimensions();
        let src_small = if sw > max_dim || sh > max_dim {
            let scale = max_dim as f32 / sw.max(sh) as f32;
            src.resize(
                (sw as f32 * scale) as u32,
                (sh as f32 * scale) as u32,
                image::imageops::FilterType::Triangle,
            )
        } else {
            src.clone()
        };
        let (gw, gh) = src_small.dimensions();

        let Ok(file) = std::fs::File::create(&path) else {
            self.status_msg = Some("Failed to create GIF file".into());
            return;
        };

        // Use gif crate directly with speed=1 (maximum NeuQuant quality)
        // image's GifEncoder defaults to speed=10 which produces poor palettes on dark images
        let mut encoder = match gif::Encoder::new(file, gw as u16, gh as u16, &[]) {
            Ok(e) => e,
            Err(e) => {
                self.status_msg = Some(format!("GIF init error: {e}"));
                return;
            }
        };
        let _ = encoder.set_repeat(gif::Repeat::Infinite);

        // Build a "base pipeline" — the static state without current animation applied.
        // For animated params, use the midpoint of low..high as the base.
        // For each frame we then override with anim.eval(t).
        let mut base_pipeline = self.pipeline.clone();
        let anim_snapshot = self.anim_states.clone();
        for (i, step) in base_pipeline.iter_mut().enumerate() {
            if let Some(step_anims) = anim_snapshot.get(i) {
                for (key, anim) in step_anims {
                    if anim.enabled {
                        // Reset to midpoint so all frames start from a neutral base
                        step.params.set_param(key, (anim.low + anim.high) * 0.5);
                    }
                }
            }
        }

        for frame_idx in 0..total_frames {
            let t = frame_idx as f64 / fps as f64;
            let mut frame_pipeline = base_pipeline.clone();
            for (i, step) in frame_pipeline.iter_mut().enumerate() {
                if let Some(step_anims) = anim_snapshot.get(i) {
                    for (key, anim) in step_anims {
                        if anim.enabled {
                            step.params.set_param(key, anim.eval(t));
                        }
                    }
                }
            }

            let pipeline = Pipeline::from_steps(&frame_pipeline);
            let result = pipeline.run(&src_small, self.gpu.as_deref());
            let mut rgba_raw = result.to_rgba8().into_raw();

            // speed=1 → best NeuQuant palette (256 neurons, slowest but highest quality)
            let mut frame = gif::Frame::from_rgba_speed(gw as u16, gh as u16, &mut rgba_raw, 1);
            frame.delay = delay_cs;

            if encoder.write_frame(&frame).is_err() {
                break;
            }
        }

        self.status_msg = Some(format!(
            "GIF saved ({total_frames} frames): {}",
            path.display()
        ));
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.sync_anim_states();
        self.maybe_dispatch_render();
        self.poll_result(ctx);
        self.update_source_texture(ctx);

        // Process file dialog results from background threads
        while let Ok(msg) = self.file_rx.try_recv() {
            match msg {
                FileMsg::OpenImage(path) => self.load_image(path),
                FileMsg::ExportImage(path) => self.export_image(path),
                FileMsg::ExportGif(path) => self.export_gif(path),
                FileMsg::SavePreset(path) => {
                    let preset = Preset {
                        steps: self.pipeline.clone(),
                    };
                    if let Ok(json) = serde_json::to_string_pretty(&preset) {
                        match std::fs::write(&path, json) {
                            Ok(_) => {
                                self.status_msg = Some(format!("Preset saved: {}", path.display()))
                            }
                            Err(e) => self.status_msg = Some(format!("Preset save error: {e}")),
                        }
                    }
                }
                FileMsg::LoadPreset(path) => {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(preset) = serde_json::from_str::<Preset>(&content) {
                            self.push_undo();
                            self.pipeline = preset.steps;
                            self.trigger_render();
                        } else {
                            self.status_msg = Some("Failed to parse preset".into());
                        }
                    }
                }
            }
        }

        // Apply animations to pipeline params
        let anim_time = ctx.input(|i| i.time);
        let mut any_anim = false;
        for (i, step) in self.pipeline.iter_mut().enumerate() {
            if let Some(step_anims) = self.anim_states.get(i) {
                for (key, anim) in step_anims {
                    if anim.enabled {
                        step.params.set_param(key, anim.eval(anim_time));
                        any_anim = true;
                    }
                }
            }
        }
        if any_anim {
            self.trigger_render();
            ctx.request_repaint();
        }

        if self.is_rendering || self.last_change.is_some() {
            ctx.request_repaint();
        }

        // Dropped files
        let drop = ctx.input(|i| {
            i.raw
                .dropped_files
                .first()
                .map(|f| (f.path.clone(), f.bytes.clone()))
        });
        if let Some((path, bytes)) = drop {
            if let Some(p) = path {
                self.load_image(p);
            } else if let Some(b) = bytes {
                self.load_image_from_bytes(&b);
            }
        }

        // Keyboard shortcuts
        let undo_pressed =
            ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Z) && !i.modifiers.shift);
        let redo_pressed = ctx.input(|i| {
            i.modifiers.ctrl
                && (i.key_pressed(egui::Key::Y)
                    || (i.key_pressed(egui::Key::Z) && i.modifiers.shift))
        });
        if undo_pressed {
            self.undo();
        }
        if redo_pressed {
            self.redo();
        }

        // ── Menu bar ─────────────────────────────────────────────────────────
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    let tx = self.file_tx.clone();
                    if ui.button("Open Image...").clicked() {
                        std::thread::spawn(move || {
                            if let Some(p) = rfd::FileDialog::new()
                                .add_filter("Image", &["png", "jpg", "jpeg", "bmp", "webp", "gif"])
                                .pick_file()
                            {
                                let _ = tx.send(FileMsg::OpenImage(p));
                            }
                        });
                        ui.close_menu();
                    }
                    ui.separator();
                    let tx2 = self.file_tx.clone();
                    if ui.button("Export PNG...").clicked() {
                        std::thread::spawn(move || {
                            if let Some(p) = rfd::FileDialog::new()
                                .add_filter("PNG", &["png"])
                                .set_file_name("output.png")
                                .save_file()
                            {
                                let _ = tx2.send(FileMsg::ExportImage(p));
                            }
                        });
                        ui.close_menu();
                    }
                    let tx3 = self.file_tx.clone();
                    if ui.button("Export JPG...").clicked() {
                        std::thread::spawn(move || {
                            if let Some(p) = rfd::FileDialog::new()
                                .add_filter("JPEG", &["jpg", "jpeg"])
                                .set_file_name("output.jpg")
                                .save_file()
                            {
                                let _ = tx3.send(FileMsg::ExportImage(p));
                            }
                        });
                        ui.close_menu();
                    }
                    let has_anim = self.has_animation();
                    let tx4 = self.file_tx.clone();
                    ui.add_enabled_ui(has_anim, |ui| {
                        if ui.button("Export GIF...").clicked() {
                            std::thread::spawn(move || {
                                if let Some(p) = rfd::FileDialog::new()
                                    .add_filter("GIF", &["gif"])
                                    .set_file_name("output.gif")
                                    .save_file()
                                {
                                    let _ = tx4.send(FileMsg::ExportGif(p));
                                }
                            });
                            ui.close_menu();
                        }
                    });
                    ui.separator();
                    let tx5 = self.file_tx.clone();
                    if ui.button("Save Preset...").clicked() {
                        std::thread::spawn(move || {
                            if let Some(p) = rfd::FileDialog::new()
                                .add_filter("JSON", &["json"])
                                .set_file_name("preset.json")
                                .save_file()
                            {
                                let _ = tx5.send(FileMsg::SavePreset(p));
                            }
                        });
                        ui.close_menu();
                    }
                    let tx6 = self.file_tx.clone();
                    if ui.button("Load Preset...").clicked() {
                        std::thread::spawn(move || {
                            if let Some(p) = rfd::FileDialog::new()
                                .add_filter("JSON", &["json"])
                                .pick_file()
                            {
                                let _ = tx6.send(FileMsg::LoadPreset(p));
                            }
                        });
                        ui.close_menu();
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui
                        .add_enabled(
                            !self.undo_stack.is_empty(),
                            egui::Button::new("Undo (Ctrl+Z)"),
                        )
                        .clicked()
                    {
                        self.undo();
                        ui.close_menu();
                    }
                    if ui
                        .add_enabled(
                            !self.redo_stack.is_empty(),
                            egui::Button::new("Redo (Ctrl+Y)"),
                        )
                        .clicked()
                    {
                        self.redo();
                        ui.close_menu();
                    }
                });
                ui.menu_button("Batch", |ui| {
                    let pipeline_snap = self.pipeline.clone();
                    let gpu_snap = self.gpu.clone();
                    if ui.button("Process folder...").clicked() {
                        std::thread::spawn(move || {
                            if let Some(input_dir) = rfd::FileDialog::new().pick_folder() {
                                if let Some(output_dir) = rfd::FileDialog::new().pick_folder() {
                                    let exts = ["png", "jpg", "jpeg", "bmp", "webp"];
                                    if let Ok(entries) = std::fs::read_dir(&input_dir) {
                                        for entry in entries.flatten() {
                                            let path = entry.path();
                                            let Some(ext) =
                                                path.extension().and_then(|e| e.to_str())
                                            else {
                                                continue;
                                            };
                                            if !exts.contains(&ext.to_lowercase().as_str()) {
                                                continue;
                                            }
                                            if let Ok(img) = image::open(&path) {
                                                let pipeline = Pipeline::from_steps(&pipeline_snap);
                                                let result =
                                                    pipeline.run(&img, gpu_snap.as_deref());
                                                let out_path = output_dir
                                                    .join(path.file_name().unwrap())
                                                    .with_extension("png");
                                                let _ = result.save(&out_path);
                                            }
                                        }
                                    }
                                }
                            }
                        });
                        ui.close_menu();
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui
                        .selectable_value(&mut self.compare_mode, CompareMode::Single, "Single")
                        .clicked()
                    {
                        ui.close_menu();
                    }
                    if ui
                        .selectable_value(
                            &mut self.compare_mode,
                            CompareMode::SideBySide,
                            "Side by Side",
                        )
                        .clicked()
                    {
                        ui.close_menu();
                    }
                });
            });
        });

        // ── Status bar ────────────────────────────────────────────────────────
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some((w, h)) = self.status_dims {
                    ui.label(format!("{}×{}", w, h));
                    ui.separator();
                }
                if self.is_rendering {
                    ui.label("Rendering...");
                } else {
                    ui.label(format!("Render: {}ms", self.status_render_ms));
                }
                if any_anim {
                    ui.separator();
                    ui.label("Animating");
                }
                ui.separator();
                ui.label(format!("Zoom: {:.0}%", self.zoom * 100.0));
                if let Some(msg) = &self.status_msg.clone() {
                    ui.separator();
                    ui.label(egui::RichText::new(msg).weak());
                }
            });
        });

        egui::SidePanel::right("pipeline_panel")
            .min_width(300.0)
            .show(ctx, |ui| {
                ui.heading("Pipeline");
                ui.separator();

                // Histogram display
                if let Some((hr, hg, hb, hl)) = &self.histogram {
                    egui::CollapsingHeader::new("Histogram")
                        .default_open(false)
                        .show(ui, |ui| {
                            let max_val = hl.iter().copied().max().unwrap_or(1).max(1);
                            let avail = ui.available_width();
                            let bar_w = (avail / 256.0).max(1.0);
                            let height = 60.0;
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(avail, height),
                                egui::Sense::hover(),
                            );
                            for i in 0..256 {
                                let x = rect.left() + i as f32 * bar_w;
                                let r_h = hr[i] as f32 / max_val as f32 * height;
                                let g_h = hg[i] as f32 / max_val as f32 * height;
                                let b_h = hb[i] as f32 / max_val as f32 * height;
                                let l_h = hl[i] as f32 / max_val as f32 * height;
                                let bottom = rect.bottom();
                                ui.painter().rect_filled(
                                    egui::Rect::from_min_max(
                                        egui::pos2(x, bottom - l_h),
                                        egui::pos2(x + bar_w, bottom),
                                    ),
                                    0.0,
                                    egui::Color32::from_rgba_unmultiplied(200, 200, 200, 60),
                                );
                                ui.painter().rect_filled(
                                    egui::Rect::from_min_max(
                                        egui::pos2(x, bottom - r_h),
                                        egui::pos2(x + bar_w, bottom),
                                    ),
                                    0.0,
                                    egui::Color32::from_rgba_unmultiplied(255, 80, 80, 100),
                                );
                                ui.painter().rect_filled(
                                    egui::Rect::from_min_max(
                                        egui::pos2(x, bottom - g_h),
                                        egui::pos2(x + bar_w, bottom),
                                    ),
                                    0.0,
                                    egui::Color32::from_rgba_unmultiplied(80, 255, 80, 100),
                                );
                                ui.painter().rect_filled(
                                    egui::Rect::from_min_max(
                                        egui::pos2(x, bottom - b_h),
                                        egui::pos2(x + bar_w, bottom),
                                    ),
                                    0.0,
                                    egui::Color32::from_rgba_unmultiplied(80, 80, 255, 100),
                                );
                            }
                        });
                }

                ui.separator();

                let mut changed = false;
                let mut to_remove: Option<usize> = None;
                let mut to_duplicate: Option<usize> = None;
                let mut swap: Option<(usize, usize, usize)> = None; // (a, b, highlight_index)

                // Animate the last-moved item highlight (500ms fade)
                let highlight_progress = self
                    .move_highlight
                    .as_ref()
                    .map(|(_, t)| {
                        let elapsed = t.elapsed().as_secs_f32();
                        (1.0 - elapsed / 0.5).clamp(0.0, 1.0)
                    })
                    .unwrap_or(0.0);
                if highlight_progress > 0.0 {
                    ctx.request_repaint();
                } else if self.move_highlight.is_some() {
                    self.move_highlight = None;
                }

                egui::ScrollArea::vertical().show(ui, |ui| {
                    let pipeline_len = self.pipeline.len();

                    for i in 0..pipeline_len {
                        let name = self.pipeline[i].params.type_name().to_string();

                        // Flash background for the recently moved item
                        let is_highlighted = self
                            .move_highlight
                            .as_ref()
                            .map(|(hi, _)| *hi == i)
                            .unwrap_or(false);
                        let stroke_color = if is_highlighted && highlight_progress > 0.0 {
                            let c = ui.visuals().selection.stroke.color;
                            egui::Color32::from_rgba_unmultiplied(
                                c.r(),
                                c.g(),
                                c.b(),
                                (highlight_progress * 180.0) as u8,
                            )
                        } else {
                            ui.visuals().widgets.noninteractive.bg_stroke.color
                        };
                        let fill_color = if is_highlighted && highlight_progress > 0.0 {
                            egui::Color32::from_rgba_unmultiplied(
                                80,
                                130,
                                255,
                                (highlight_progress * 40.0) as u8,
                            )
                        } else {
                            egui::Color32::TRANSPARENT
                        };

                        egui::Frame::default()
                            .stroke(egui::Stroke::new(1.0, stroke_color))
                            .fill(fill_color)
                            .inner_margin(6.0)
                            .outer_margin(egui::Margin {
                                bottom: 4,
                                ..Default::default()
                            })
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    if ui.checkbox(&mut self.pipeline[i].enabled, "").changed() {
                                        changed = true;
                                    }
                                    ui.strong(format!("{}. {}", i + 1, name));
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.small_button("x").clicked() {
                                                to_remove = Some(i);
                                            }
                                            if ui.small_button("+").clicked() {
                                                to_duplicate = Some(i);
                                            }
                                            ui.add_enabled_ui(i < pipeline_len - 1, |ui| {
                                                if ui.small_button("v").clicked() {
                                                    swap = Some((i, i + 1, i + 1));
                                                }
                                            });
                                            ui.add_enabled_ui(i > 0, |ui| {
                                                if ui.small_button("^").clicked() {
                                                    swap = Some((i - 1, i, i - 1));
                                                }
                                            });
                                        },
                                    );
                                });
                                // Opacity slider
                                ui.horizontal(|ui| {
                                    ui.label("Opacity:");
                                    if ui
                                        .add(
                                            egui::Slider::new(
                                                &mut self.pipeline[i].opacity,
                                                0.0..=1.0,
                                            )
                                            .clamping(egui::SliderClamping::Always),
                                        )
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                });

                                egui::CollapsingHeader::new("Settings").show(ui, |ui| {
                                    if show_effect_params(ui, &mut self.pipeline[i].params) {
                                        changed = true;
                                    }
                                });

                                // Animate panel — one row per animatable param
                                let anim_params = self.pipeline[i].params.animatable_params();
                                if !anim_params.is_empty() {
                                    egui::CollapsingHeader::new("Animate").show(ui, |ui| {
                                        for (key, _current, min, max) in anim_params {
                                            let step_anims = &mut self.anim_states[i];
                                            let anim =
                                                step_anims.entry(key.to_string()).or_insert_with(
                                                    || AnimParam::new(_current, min, max),
                                                );
                                            ui.horizontal(|ui| {
                                                ui.checkbox(&mut anim.enabled, key);
                                            });
                                            if anim.enabled {
                                                ui.indent(key, |ui| {
                                                    ui.horizontal(|ui| {
                                                        ui.label("Speed:");
                                                        ui.add(
                                                            egui::Slider::new(
                                                                &mut anim.speed,
                                                                0.05..=5.0,
                                                            )
                                                            .suffix("×"),
                                                        );
                                                    });
                                                    ui.horizontal(|ui| {
                                                        ui.label("Low:");
                                                        ui.add(egui::Slider::new(
                                                            &mut anim.low,
                                                            min..=max,
                                                        ));
                                                    });
                                                    ui.horizontal(|ui| {
                                                        ui.label("High:");
                                                        ui.add(egui::Slider::new(
                                                            &mut anim.high,
                                                            min..=max,
                                                        ));
                                                    });
                                                });
                                            }
                                        }
                                    });
                                }
                            });
                    }

                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("+ Add Effect").clicked() {
                            self.show_add_effect = !self.show_add_effect;
                        }
                    });

                    if self.show_add_effect {
                        let effect_types = [
                            ("Transform", EffectParams::Transform(Default::default())),
                            (
                                "Color Adjust",
                                EffectParams::ColorAdjust(Default::default()),
                            ),
                            ("Pixel Sort", EffectParams::PixelSort(Default::default())),
                            ("Dither", EffectParams::Dither(Default::default())),
                            (
                                "Color Quantization",
                                EffectParams::Quantize(Default::default()),
                            ),
                            ("Databend", EffectParams::Databend(Default::default())),
                            ("Halftone", EffectParams::Halftone(Default::default())),
                            (
                                "Chromatic Aberration",
                                EffectParams::ChromaticAberration(Default::default()),
                            ),
                            ("Pixelate", EffectParams::Pixelate(Default::default())),
                            ("Noise", EffectParams::Noise(Default::default())),
                            ("Bloom", EffectParams::Bloom(Default::default())),
                            ("Kuwahara", EffectParams::Kuwahara(Default::default())),
                            ("VHS / CRT", EffectParams::Vhs(Default::default())),
                        ];
                        for (label, params) in effect_types {
                            if ui.button(label).clicked() {
                                self.push_undo();
                                self.pipeline.push(PipelineStep::new(params));
                                self.show_add_effect = false;
                                changed = true;
                            }
                        }
                    }

                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Seed:");
                        let seed_str = &mut format!("{}", self.randomize_seed);
                        if ui.text_edit_singleline(seed_str).changed() {
                            if let Ok(v) = seed_str.parse::<u64>() {
                                self.randomize_seed = v;
                            }
                        }
                        if ui.button("Randomize All").clicked() {
                            self.push_undo();
                            randomize_pipeline(&mut self.pipeline, self.randomize_seed);
                            changed = true;
                        }
                    });
                });

                if let Some(i) = to_remove {
                    self.push_undo();
                    self.pipeline.remove(i);
                    changed = true;
                }
                if let Some(i) = to_duplicate {
                    self.push_undo();
                    let step = self.pipeline[i].clone();
                    self.pipeline.insert(i + 1, step);
                    changed = true;
                }
                if let Some((a, b, highlight)) = swap {
                    self.push_undo();
                    self.pipeline.swap(a, b);
                    self.move_highlight = Some((highlight, Instant::now()));
                    changed = true;
                }
                if changed {
                    self.trigger_render();
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.source_image.is_none() {
                // Highlight when a file is being dragged over the window
                let is_hovering_file = ctx.input(|i| !i.raw.hovered_files.is_empty());
                let bg_color = if is_hovering_file {
                    egui::Color32::from_rgba_unmultiplied(100, 150, 255, 30)
                } else {
                    egui::Color32::TRANSPARENT
                };
                let rect = ui.max_rect();
                ui.painter().rect_filled(rect, 8.0, bg_color);
                if is_hovering_file {
                    ui.painter().rect_stroke(
                        rect.shrink(4.0),
                        8.0,
                        egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 150, 255)),
                        egui::StrokeKind::Outside,
                    );
                }
                ui.centered_and_justified(|ui| {
                    let msg = if is_hovering_file {
                        "Release to open image"
                    } else {
                        "Drop an image here or use File -> Open"
                    };
                    ui.label(egui::RichText::new(msg).size(18.0));
                });
                return;
            }

            let available = ui.available_size();

            // Zoom with scroll
            let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll_delta != 0.0 {
                self.zoom = (self.zoom * (1.0 + scroll_delta * 0.002)).clamp(0.05, 20.0);
            }

            // Pan with drag
            if ui.input(|i| i.pointer.secondary_down()) {
                let delta = ui.input(|i| i.pointer.delta());
                self.pan += delta;
            }

            // Reset zoom on double click
            if ui.input(|i| {
                i.pointer
                    .button_double_clicked(egui::PointerButton::Primary)
            }) {
                self.zoom = 1.0;
                self.pan = Vec2::ZERO;
            }

            match self.compare_mode {
                CompareMode::Single => {
                    if let Some(tex) = &self.result_texture {
                        let size = tex.size_vec2() * self.zoom;
                        let offset = (available - size) * 0.5 + self.pan;
                        let rect = Rect::from_min_size(ui.min_rect().min + offset, size);
                        ui.painter().image(
                            tex.id(),
                            rect,
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                    }
                }
                CompareMode::SideBySide => {
                    let half = available.x / 2.0;
                    let src_tex = self.source_texture.clone();
                    let res_tex = self.result_texture.clone();
                    if let (Some(src), Some(res)) = (src_tex, res_tex) {
                        let size = src.size_vec2() * self.zoom;
                        let left_offset =
                            Vec2::new(half / 2.0 - size.x / 2.0, (available.y - size.y) / 2.0)
                                + self.pan;
                        let right_offset = Vec2::new(
                            half + half / 2.0 - size.x / 2.0,
                            (available.y - size.y) / 2.0,
                        ) + self.pan;
                        let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                        let src_rect = Rect::from_min_size(ui.min_rect().min + left_offset, size);
                        let res_rect = Rect::from_min_size(ui.min_rect().min + right_offset, size);
                        ui.painter()
                            .image(src.id(), src_rect, uv, egui::Color32::WHITE);
                        ui.painter()
                            .image(res.id(), res_rect, uv, egui::Color32::WHITE);
                        let mid_x = ui.min_rect().min.x + half;
                        ui.painter().line_segment(
                            [
                                Pos2::new(mid_x, ui.min_rect().min.y),
                                Pos2::new(mid_x, ui.min_rect().max.y),
                            ],
                            egui::Stroke::new(1.0, egui::Color32::GRAY),
                        );
                    }
                }
            }
        });
    }
}

fn lcg_next(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *state
}

fn rng_f(state: &mut u64, min: f32, max: f32) -> f32 {
    let v = lcg_next(state);
    let t = (v >> 33) as f32 / (u32::MAX as f32);
    min + t * (max - min)
}

fn rng_u(state: &mut u64, max: u64) -> u64 {
    lcg_next(state) % max
}

fn randomize_pipeline(steps: &mut Vec<PipelineStep>, seed: u64) {
    let mut rng = seed.wrapping_add(1);

    for step in steps.iter_mut() {
        match &mut step.params {
            EffectParams::PixelSort(p) => {
                p.threshold_low = rng_f(&mut rng, 0.0, 0.4);
                p.threshold_high = rng_f(&mut rng, 0.6, 1.0);
                p.ascending = rng_u(&mut rng, 2) == 0;
                p.segment_size = rng_u(&mut rng, 500) as usize + 10;
            }
            EffectParams::Dither(p) => {
                p.scale_factor = rng_u(&mut rng, 4) as u32 + 1;
            }
            EffectParams::Quantize(p) => {
                p.num_colors = rng_u(&mut rng, 30) as usize + 2;
            }
            EffectParams::Databend(p) => {
                p.intensity = rng_f(&mut rng, 0.01, 0.3);
                p.seed = rng_u(&mut rng, u64::MAX);
            }
            EffectParams::Halftone(p) => {
                p.dot_size = rng_f(&mut rng, 3.0, 20.0);
                p.spacing = rng_f(&mut rng, 5.0, 25.0);
                p.angle = rng_f(&mut rng, 0.0, 90.0);
            }
            EffectParams::ChromaticAberration(p) => {
                p.r_dx = rng_u(&mut rng, 20) as i32 - 10;
                p.r_dy = rng_u(&mut rng, 10) as i32 - 5;
                p.b_dx = rng_u(&mut rng, 20) as i32 - 10;
                p.b_dy = rng_u(&mut rng, 10) as i32 - 5;
            }
            EffectParams::ColorAdjust(p) => {
                p.brightness = rng_f(&mut rng, -0.3, 0.3);
                p.contrast = rng_f(&mut rng, -0.3, 0.3);
                p.saturation = rng_f(&mut rng, -0.5, 0.5);
                p.hue_shift = rng_f(&mut rng, -180.0, 180.0);
                p.temperature = rng_f(&mut rng, -0.5, 0.5);
            }
            EffectParams::Transform(p) => {
                p.angle = rng_f(&mut rng, -45.0, 45.0);
                p.flip_h = rng_u(&mut rng, 2) == 0;
                p.flip_v = rng_u(&mut rng, 2) == 0;
            }
            EffectParams::Pixelate(p) => {
                p.block_w = rng_u(&mut rng, 50) as u32 + 2;
                p.block_h = p.block_w;
            }
            EffectParams::Noise(p) => {
                p.intensity = rng_f(&mut rng, 0.02, 0.4);
                p.seed = rng_u(&mut rng, u64::MAX);
            }
            EffectParams::Bloom(p) => {
                p.threshold = rng_f(&mut rng, 0.3, 0.9);
                p.intensity = rng_f(&mut rng, 0.3, 2.0);
                p.radius = rng_f(&mut rng, 3.0, 30.0);
            }
            EffectParams::Kuwahara(p) => {
                p.radius = rng_u(&mut rng, 10) as i32 + 2;
            }
            EffectParams::Vhs(p) => {
                p.scanline_intensity = rng_f(&mut rng, 0.1, 0.5);
                p.color_bleed = rng_f(&mut rng, 1.0, 10.0);
                p.noise = rng_f(&mut rng, 0.01, 0.2);
                p.vignette = rng_f(&mut rng, 0.1, 0.7);
                p.barrel = rng_f(&mut rng, 0.0, 0.15);
            }
        }
    }
}
