use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

use perfetto_everywhere_collector::{Collector, CollectorConfig, RealmDescriptor};
use perfetto_everywhere_core::{Category, MetadataDef, StaticName, Tracer, TrackId};
use perfetto_everywhere_raw::RawRingBackend;
use perfetto_everywhere_web::{
    ClockCalibration, MetadataEntry, OrdinaryBackend, ProducerHealth, SourceClock,
};

const WINDOW: Category = Category::new("frontend");
const WINDOW_SPAN: StaticName = StaticName::new("frontend.egui.update");
const CALLBACK: StaticName = StaticName::new("engine.rt.callback");
const CALLBACK_EVENT: StaticName = StaticName::new("engine.callback");
const LOAD: StaticName = StaticName::new("engine.callback.load");

#[derive(Clone)]
struct CellClock(Rc<Cell<u64>>);

impl SourceClock for CellClock {
    fn now_ticks(&self) -> Option<u64> {
        Some(self.0.get())
    }
}

fn metadata(definition: MetadataDef) -> MetadataEntry {
    MetadataEntry {
        id: definition.id,
        namespace: definition.namespace,
        label: definition.label.to_owned(),
    }
}

fn static_metadata(name: StaticName) -> MetadataEntry {
    metadata(MetadataDef {
        id: name.id,
        namespace: 1,
        label: name.label,
    })
}

fn category_metadata(category: Category) -> MetadataEntry {
    metadata(MetadataDef {
        id: category.id,
        namespace: 2,
        label: category.label,
    })
}

fn raw_realm(realm_id: u32, clock_id: u32, begin: u64, end: u64) -> (Vec<u8>, ProducerHealth) {
    let backend = RawRingBackend::new(realm_id, clock_id, 64, &[]).unwrap();
    backend.set_timestamp(begin);
    let tracer = Tracer::new(backend);
    let span = tracer.span(Category::new("shoop.realtime"), CALLBACK, &[]);
    tracer.backend().set_timestamp(end);
    drop(span);
    let _ = tracer.event(Category::new("shoop.realtime"), CALLBACK_EVENT, &[]);
    let _ = tracer.counter_f64(LOAD, TrackId::CURRENT, 0.25);
    let mut bytes = vec![0; tracer.backend().available_records() * 48];
    let initialized = tracer.backend().drain_into(&mut bytes);
    bytes.truncate(initialized);
    let health = tracer.backend().health();
    (
        bytes,
        ProducerHealth {
            emitted_records: health.emitted_records,
            dropped_records: health.dropped_records,
            completed_batches: health.completed_drains,
            high_water_records: health.high_water_records,
            repaired_span_boundaries: 0,
            ..ProducerHealth::default()
        },
    )
}

fn main() {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("artifacts/shoop-multirealm.pftrace"));
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }

    let window_clock = CellClock(Rc::new(Cell::new(100)));
    let window_backend = OrdinaryBackend::new(1, 101, window_clock.clone(), 64, &[]).unwrap();
    let window_tracer = Tracer::new(window_backend);
    {
        let _span = window_tracer.span(WINDOW, WINDOW_SPAN, &[]);
        window_clock.0.set(200);
    }
    let window_records = window_tracer.backend().flush_and_take_batch().unwrap();
    let window_metadata = window_tracer.backend().take_metadata();
    let window_health = window_tracer.backend().health();

    let (worker_records, worker_health) = raw_realm(2, 102, 128, 256);
    let (audio_records, audio_health) = raw_realm(4, 104, 256, 384);
    let raw_metadata = [
        category_metadata(Category::new("shoop.realtime")),
        static_metadata(CALLBACK),
        static_metadata(StaticName::new("span end")),
        static_metadata(CALLBACK_EVENT),
        static_metadata(LOAD),
    ];

    let mut collector = Collector::new(CollectorConfig::default());
    for (id, label, ticks) in [
        (1, "Window", 1_000_000_000),
        (2, "Engine Worker", 48_000),
        (4, "AudioWorklet", 48_000),
    ] {
        collector
            .register_realm(RealmDescriptor {
                id,
                label: label.to_owned(),
                ticks_per_second: ticks,
            })
            .unwrap();
    }
    collector.register_metadata_all(window_metadata).unwrap();
    collector
        .register_metadata_all(raw_metadata.iter().cloned())
        .unwrap();
    for sample in [
        ClockCalibration {
            realm_id: 1,
            clock_id: 101,
            source_ticks: 1,
            reference_time_ns: 1_000_000_001,
            uncertainty_ns: 1,
        },
        ClockCalibration {
            realm_id: 1,
            clock_id: 101,
            source_ticks: 1_000,
            reference_time_ns: 1_000_001_000,
            uncertainty_ns: 1,
        },
        ClockCalibration {
            realm_id: 2,
            clock_id: 102,
            source_ticks: 1,
            reference_time_ns: 1_000_020_833,
            uncertainty_ns: 1,
        },
        ClockCalibration {
            realm_id: 2,
            clock_id: 102,
            source_ticks: 48_000,
            reference_time_ns: 2_000_000_000,
            uncertainty_ns: 1,
        },
        ClockCalibration {
            realm_id: 4,
            clock_id: 104,
            source_ticks: 1,
            reference_time_ns: 1_000_020_833,
            uncertainty_ns: 1,
        },
        ClockCalibration {
            realm_id: 4,
            clock_id: 104,
            source_ticks: 48_000,
            reference_time_ns: 2_000_000_000,
            uncertainty_ns: 1,
        },
    ] {
        collector.add_calibration(sample).unwrap();
    }
    collector.ingest_batch(&window_records).unwrap();
    collector.ingest_batch(&worker_records).unwrap();
    collector.ingest_batch(&audio_records).unwrap();
    collector.set_health(1, window_health);
    collector.set_health(2, worker_health);
    collector.set_health(4, audio_health);
    std::fs::write(&output, collector.finish().unwrap()).unwrap();
    println!("{}", output.display());
}
