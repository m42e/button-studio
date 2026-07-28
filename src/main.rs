use app::ButtonStudio;

mod app;
mod fonts;
mod model;
mod render;
mod series;
mod theme;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Pincraft Button Studio")
            .with_inner_size([1_440.0, 900.0])
            .with_min_inner_size([1_120.0, 720.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Pincraft Button Studio",
        options,
        Box::new(|context| Ok(Box::new(ButtonStudio::new(context)))),
    )
}
