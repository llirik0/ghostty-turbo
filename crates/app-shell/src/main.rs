mod app;

fn main() -> eframe::Result {
    let viewport = eframe::egui::ViewportBuilder::default()
        .with_title("Ghostty Shell")
        .with_inner_size([1480.0, 920.0])
        .with_min_inner_size([1160.0, 760.0]);

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "ghostty-shell",
        options,
        Box::new(|cc| Ok(Box::new(app::GhosttyShellApp::new(cc)))),
    )
}

