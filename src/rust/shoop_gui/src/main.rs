//! Starts the window.
//!
//! Everything of substance is in the library beside this, so it can be tested without a
//! display. This only opens a window and hands over.

use shoop_gui::app;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 700.0])
            .with_title("ShoopDaLoop (Rust engine)"),
        ..Default::default()
    };

    eframe::run_native(
        "shoop-gui",
        options,
        // Failing to open a device is reported in the window rather than aborting: it is
        // the most likely thing to go wrong and the least useful thing to crash on.
        Box::new(|_cc| Ok(Box::new(app::App::new()))),
    )
}
