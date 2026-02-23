mod affinity;
mod app;
mod audio;
mod module;
mod modules;
mod reactor;
mod recorder;
mod renderer;
mod sdf;
mod substrate;
mod tuning;

fn main() -> eframe::Result {
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Solido v0.6")
            .with_inner_size([1200.0, 700.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "solido",
        options,
        Box::new(|cc| Ok(Box::new(app::SolidoApp::new(cc)))),
    )
}
