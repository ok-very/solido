mod affinity;
mod app;
mod audio;
mod dsp;
mod module;
mod modules;
mod organism;
mod preset;
mod reactor;
mod recorder;
mod renderer;
mod sdf;
mod substrate;
mod tuning;
mod ui;

fn main() -> eframe::Result {
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Solido v0.6")
            .with_decorations(false)
            .with_transparent(true)
            .with_inner_size([1200.0, 700.0])
            .with_min_inner_size([800.0, 500.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "solido",
        options,
        Box::new(|cc| Ok(Box::new(app::SolidoApp::new(cc)))),
    )
}
