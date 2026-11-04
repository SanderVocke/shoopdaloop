use crate::ProfilingView;

pub(crate) fn show(ui: &mut egui::Ui, profiling: &ProfilingView, capture: &mut bool) {
    if !profiling.available {
        ui.label("Elapsed-time profiling is unavailable on this host.");
    }
    ui.label(if profiling.requested && !profiling.available {
        "Profiling requested, but no elapsed-time clock is available."
    } else if profiling.requested != profiling.active {
        "Waiting for the audio callback to apply profiling."
    } else if profiling.active {
        "Profiling is on."
    } else {
        "Profiling is off."
    });
    if profiling.available {
        if profiling.resolution_ns == 0 {
            ui.weak("Clock resolution is unknown.");
        } else {
            ui.weak(format!(
                "Observed clock step ≈ {:.3} µs",
                profiling.resolution_ns as f64 / 1000.
            ));
        }
    }
    ui.horizontal(|ui| {
        ui.selectable_value(capture, false, "Live");
        ui.add_enabled_ui(profiling.capture.is_some(), |ui| {
            ui.selectable_value(capture, true, "Latest xrun");
        });
    });
    let report = if *capture {
        match &profiling.capture {
            Some(report) => report,
            None => {
                *capture = false;
                &profiling.live
            }
        }
    } else {
        &profiling.live
    };
    if report.rows.is_empty() {
        ui.label("No measured graph node samples in this interval.");
        return;
    }
    ui.label(format!(
        "Measured interval: {:.3}–{:.3} ms · frames {}–{} · generation {}",
        report.interval.start_ns as f64 / 1e6,
        report.interval.end_ns as f64 / 1e6,
        report.interval.first_frame,
        report.interval.end_frame,
        report.generation
    ));
    ui.label("Node duration includes input preparation, routing and processing. Values are in µs.");
    if report.incomplete || profiling.dropped != 0 {
        ui.colored_label(
            crate::colors::WARNING,
            format!(
                "Incomplete timing data; {} live batches lost.",
                profiling.dropped
            ),
        );
    }
    if profiling.skipped_captures != 0 {
        ui.label(format!(
            "{} additional xrun captures could not be retained.",
            profiling.skipped_captures
        ));
    }
    if report.capture != 0 {
        ui.label(format!(
            "Capture {} · {}",
            report.capture,
            if report.complete {
                "complete"
            } else {
                "receiving"
            }
        ));
        if report.trigger & 1 != 0 {
            ui.label("Host-reported xrun; its callback attribution may be delayed.");
        }
        if report.trigger & 2 != 0 {
            ui.label("Measured callback budget overrun.");
        }
    }
    egui::Grid::new("runtime_profiling_table")
        .striped(true)
        .show(ui, |ui| {
            for title in ["Node", "Avg", "Worst", "Stddev"] {
                ui.strong(title);
            }
            ui.end_row();
            for row in &report.rows {
                ui.label(&row.name);
                if row.samples == 0 {
                    for _ in 0..3 {
                        ui.label("—");
                    }
                } else {
                    ui.monospace(format!("{:.3}", row.avg_ns / 1000.));
                    ui.monospace(format!("{:.3}", row.worst_ns as f64 / 1000.));
                    ui.monospace(format!("{:.3}", row.stddev_ns / 1000.));
                }
                ui.end_row();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;

    fn texts(shape: &egui::Shape, output: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text) => output.push(text.galley.job.text.clone()),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    texts(shape, output);
                }
            }
            _ => {}
        }
    }
    #[shoop_test]
    fn table_renders_known_statistics_and_clears_on_generation_reset() {
        let context = egui::Context::default();
        let mut profiling = ProfilingView {
            generation: 1,
            available: true,
            ..Default::default()
        };
        profiling.live.rows.push(Default::default());
        let row = &mut profiling.live.rows[0];
        row.name = "Loop 1 MIDI playback".into();
        row.samples = 4;
        row.avg_ns = 1250.;
        row.worst_ns = 3000;
        row.stddev_ns = 500.;
        let render = |profiling: &ProfilingView, mut capture: bool| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200., 900.),
                    )),
                    ..Default::default()
                },
                |ui| show(ui, profiling, &mut capture),
            );
            output.textures_delta.clear();
            let mut labels = Vec::new();
            for shape in output.shapes {
                texts(&shape.shape, &mut labels);
            }
            labels
        };
        let labels = render(&profiling, false);
        for expected in [
            "Node",
            "Avg",
            "Worst",
            "Stddev",
            "Loop 1 MIDI playback",
            "1.250",
            "3.000",
            "0.500",
        ] {
            assert!(
                labels.iter().any(|text| text == expected),
                "missing {expected}: {labels:?}"
            );
        }
        profiling.capture = Some(profiling.live.clone());
        profiling.capture.as_mut().unwrap().rows[0].name = "Captured Loop 1".into();
        let labels = render(&profiling, true);
        assert!(labels.iter().any(|text| text == "Captured Loop 1"));
        assert!(!labels.iter().any(|text| text == "Loop 1 MIDI playback"));
        profiling.generation = 2;
        profiling.live.rows.clear();
        profiling.capture = None;
        profiling.available = false;
        profiling.requested = true;
        let labels = render(&profiling, false);
        assert!(!labels
            .iter()
            .any(|text| text.contains("Loop 1 MIDI playback")));
        assert!(labels.iter().any(|text| text.contains("No measured")));
        assert!(labels
            .iter()
            .any(|text| text.contains("no elapsed-time clock")));
    }
}
