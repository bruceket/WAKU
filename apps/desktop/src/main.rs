//! `waku`: app desktop G0 (eframe + wgpu).

#![cfg_attr(not(test), windows_subsystem = "windows")]

mod ai_access;
mod app;
mod gui;
mod plan_view;
mod theme;
mod tools;
mod view3d;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        depth_buffer: 32,
        viewport: egui::ViewportBuilder::default()
            .with_title("Waku")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Waku",
        options,
        Box::new(|cc| Ok(Box::new(gui::WakuApp::new(cc)?))),
    )
}
