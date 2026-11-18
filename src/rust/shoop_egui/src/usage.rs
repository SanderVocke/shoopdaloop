use crate::colors;
use shoop_app_api::{MemoryReport, MemoryResource, MemoryUnit};

const COVERAGE: &str =
    "Includes prepared media storage, metadata and scratch. Shared payload is counted once.";

fn bytes(value: u64) -> String {
    for (shift, unit) in [(30, "GiB"), (20, "MiB"), (10, "KiB")] {
        if value >= 1 << shift {
            return format!("{:.1} {unit}", value as f64 / (1u64 << shift) as f64);
        }
    }
    format!("{value} B")
}

fn percent(pressure: f64) -> String {
    if pressure < 1. && pressure >= 0.9995 {
        return ">99.9%".into();
    }
    format!("{:.1}%", 100. * pressure)
}

pub(crate) fn summary(ui: &mut egui::Ui, report: Option<&MemoryReport>) {
    let limiting = report.and_then(MemoryReport::limiting_resource);
    let value = limiting
        .and_then(MemoryResource::pressure)
        .map(percent)
        .unwrap_or_else(|| "—".into());
    ui.label(format!("Usage {value}")).on_hover_ui(|ui| {
        if let Some(resource) = limiting {
            ui.strong(format!("Closest to capacity · {}", resource.name.as_str()));
        }
        details(ui, report);
    });
}

pub fn details(ui: &mut egui::Ui, report: Option<&MemoryReport>) {
    let Some(report) = report else {
        ui.weak("Resource usage unavailable");
        return;
    };
    if let Some(accounted) = report.accounted_bytes {
        ui.label(format!("Accounted backend memory · {}", bytes(accounted)))
            .on_hover_text(COVERAGE);
    }
    if report.resources.iter().all(Option::is_none) {
        ui.weak("Resource limits unavailable");
        return;
    }
    let limiting = report.limiting_resource();
    egui::Grid::new("usage_resources")
        .striped(true)
        .show(ui, |ui| {
            for heading in ["Resource", "Used", "Reserved", "Capacity", "Free", "Usage"] {
                ui.strong(heading);
            }
            ui.end_row();
            for resource in report.resources.iter().flatten() {
                let format = |value| match resource.unit {
                    MemoryUnit::Bytes => bytes(value),
                    MemoryUnit::Entries => value.to_string(),
                    MemoryUnit::Pages => format!("{value} pages"),
                    MemoryUnit::Blocks => format!("{value} blocks"),
                };
                let is_limiting = limiting == Some(resource);
                let name = egui::RichText::new(resource.name.as_str());
                let label = ui.label(if is_limiting { name.strong() } else { name });
                if is_limiting {
                    label.on_hover_text("Closest to capacity");
                }
                ui.monospace(format(resource.used));
                ui.monospace(format(resource.reserved))
                    .on_hover_text("Capacity set aside for an owner or a pending program");
                if let Some(capacity) = resource.capacity.filter(|&capacity| capacity > 0) {
                    ui.monospace(format(capacity));
                    ui.monospace(format(
                        capacity.saturating_sub(resource.used.saturating_add(resource.reserved)),
                    ));
                    usage_bar(ui, resource, capacity, is_limiting);
                } else {
                    ui.weak("Unknown");
                    ui.weak("—");
                    ui.weak("—");
                }
                ui.end_row();
            }
        });
}

fn usage_bar(ui: &mut egui::Ui, resource: &MemoryResource, capacity: u64, limiting: bool) {
    let committed = resource.pressure().unwrap_or(0.);
    let used = resource.used as f64 / capacity as f64;
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(74., 8.), egui::Sense::hover());
        let painter = ui.painter();
        let end = |fraction: f64| rect.left() + rect.width() * fraction.clamp(0., 1.) as f32;
        painter.rect_filled(rect, 0., ui.visuals().widgets.inactive.bg_fill);
        let fill = if committed >= 1. {
            colors::ERROR
        } else if limiting {
            colors::COLORED_HIGHLIGHT
        } else {
            colors::MUTED_SLIDER_FILL
        };
        painter.rect_filled(rect.with_max_x(end(used)), 0., fill);
        if resource.reserved > 0 {
            let reserved = rect.with_min_x(end(used)).with_max_x(end(committed));
            painter.rect_filled(reserved, 0., colors::WARNING);
        }
        ui.monospace(percent(committed));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;

    fn resource(name: &str, used: u64, reserved: u64, capacity: Option<u64>) -> MemoryResource {
        MemoryResource {
            name: shoop_app_api::diagnostics_protocol::Name::new(name).unwrap(),
            unit: MemoryUnit::Pages,
            used,
            reserved,
            capacity,
        }
    }

    #[shoop_test]
    fn usage_summary_prefers_the_limiting_pool_and_distinguishes_zero_from_unavailable() {
        let context = egui::Context::default();
        let mut bounded = MemoryReport {
            accounted_bytes: Some(4096),
            ..Default::default()
        };
        bounded.resources[0] = Some(resource("Audio", 85, 0, Some(100)));
        bounded.resources[1] = Some(resource("State", 75, 20, Some(100)));
        bounded.resources[2] = Some(resource("Footprint", 200, 0, None));
        let mut empty = MemoryReport::default();
        empty.resources[0] = Some(resource("Empty", 0, 0, Some(100)));
        for (report, expected) in [
            (Some(bounded), "Usage 95.0%"),
            (Some(empty), "Usage 0.0%"),
            (None, "Usage —"),
            (
                Some(MemoryReport {
                    accounted_bytes: Some(0),
                    ..Default::default()
                }),
                "Usage —",
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
