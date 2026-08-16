use crate::colors;

pub(crate) fn dial_indicator(rect: egui::Rect, fraction: f32) -> [egui::Pos2; 2] {
    let angle = -2.35 + fraction.clamp(0.0, 1.0) * 4.7;
    let direction = egui::vec2(angle.sin(), -angle.cos());
    [
        rect.center() + direction * rect.width() * 0.30,
        rect.center() + direction * rect.width() * 0.43,
    ]
}

pub(crate) fn paint_dial(
    ui: &egui::Ui,
    response: &egui::Response,
    rect: egui::Rect,
    fraction: f32,
    label: &str,
) {
    let _visuals = ui.style().interact(response);
    ui.painter().circle_filled(
        rect.center(),
        rect.width() / 2.0,
        colors::CONTROL_BACKGROUND,
    );
    ui.painter().circle_stroke(
        rect.center(),
        rect.width() / 2.0,
        egui::Stroke::new(1.0, colors::MUTED_FOREGROUND),
    );
    ui.painter().line_segment(
        dial_indicator(rect, fraction),
        egui::Stroke::new(2.0, colors::COLORED_HIGHLIGHT),
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(7.0),
        colors::DIAL_LABEL,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[shoop_wasm_test_support::shoop_test]
    fn dial_indicator_stays_outside_the_label_area() {
        let rect = egui::Rect::from_center_size(egui::pos2(20.0, 20.0), egui::vec2(18.0, 18.0));
        for fraction in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let segment = dial_indicator(rect, fraction);
            assert!(segment[0].distance(rect.center()) >= rect.width() * 0.29);
            assert!(segment[1].distance(rect.center()) > segment[0].distance(rect.center()));
        }
    }
}
