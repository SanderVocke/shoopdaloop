use shoop_app_api::{MemoryReport, MemoryUnit};

const COVERAGE: &str = "Accounted backend resource storage: audio/MIDI payload, prepared spare capacity, storage metadata and scratch. Excludes allocator overhead, graph/control queues, host buffers, processors and UI memory. Shared payload is counted once. Pool reservations are unused capacity committed to specific owners.";

fn bytes(value: u64) -> String {
    for (shift, unit) in [(30, "GiB"), (20, "MiB"), (10, "KiB")] {
        if value >= 1 << shift {
            return format!("{:.1} {unit}", value as f64 / (1u64 << shift) as f64);
        }
    }
    format!("{value} B")
}

pub(crate) fn summary(ui: &mut egui::Ui, report: Option<&MemoryReport>) {
    let value = report
        .and_then(|report| report.accounted_bytes)
        .map(bytes)
        .unwrap_or_else(|| "—".into());
    ui.label(format!("MEM {value}"))
        .on_hover_ui(|ui| details(ui, report));
}

pub fn details(ui: &mut egui::Ui, report: Option<&MemoryReport>) {
    ui.strong("Accounted backend memory");
    ui.label(COVERAGE);
    let Some(report) = report else {
        ui.weak("Memory statistics unavailable");
        return;
    };
    ui.label(
        report
            .accounted_bytes
            .map(bytes)
            .unwrap_or_else(|| "Footprint unavailable".into()),
    );
    egui::Grid::new("memory_resources")
        .striped(true)
        .show(ui, |ui| {
            ui.strong("Resource");
            ui.strong("Used");
            ui.strong("Reserved");
            ui.strong("Capacity");
            ui.end_row();
            for resource in report.resources.iter().flatten() {
                let format = |value| match resource.unit {
                    MemoryUnit::Bytes => bytes(value),
                    MemoryUnit::Entries => format!("{value} entries"),
                    MemoryUnit::Pages => format!("{value} pages"),
                    MemoryUnit::Blocks => format!("{value} blocks"),
                };
                ui.label(resource.name.as_str());
                ui.monospace(format(resource.used));
                ui.monospace(format(resource.reserved));
                ui.monospace(
                    resource
                        .capacity
                        .map(format)
                        .unwrap_or_else(|| "No known ceiling".into()),
                );
                ui.end_row();
            }
        });
    if let Some(resource) = report.limiting_resource() {
        ui.small(format!(
            "Highest pool commitment: {} · {:.1}%",
            resource.name.as_str(),
            100. * resource.pressure().unwrap_or(0.)
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test]
    fn unavailable_and_empty_memory_are_distinct_in_the_display() {
        let context = egui::Context::default();
        for (report, expected) in [
            (None, "MEM —"),
            (
                Some(MemoryReport {
                    accounted_bytes: Some(0),
                    ..Default::default()
                }),
                "MEM 0 B",
            ),
        ] {
            let mut output = context.run_ui(Default::default(), |ui| summary(ui, report.as_ref()));
            output.textures_delta.clear();
            assert!(output.shapes.iter().any(|shape| match &shape.shape {
                egui::Shape::Text(text) => text.galley.text() == expected,
                _ => false,
            }));
        }
    }
}
