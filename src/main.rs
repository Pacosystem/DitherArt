mod effects;
mod pipeline;
mod gpu;
mod anim;
mod app;
mod ui;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("DitherArt")
            .with_inner_size([1280.0, 800.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    eframe::run_native(
        "DitherArt",
        native_options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
