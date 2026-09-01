use std::rc::Rc;

use perfetto_everywhere_core::{Category, FieldName, StaticName};
use perfetto_everywhere_raw::RawRingBackend;

use crate::{RAW_BACKEND, REALTIME_CATEGORY};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TraceMetadata {
    pub id: u32,
    pub namespace: u8,
    pub label: &'static str,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RawProducerHealth {
    pub emitted_records: u64,
    pub dropped_records: u64,
    pub completed_drains: u64,
    pub high_water_records: usize,
}

pub struct RawTraceProducer {
    backend: Rc<RawRingBackend>,
}

impl RawTraceProducer {
    pub fn new(
        realm_id: u32,
        clock_id: u32,
        capacity_records: usize,
    ) -> Result<Self, &'static str> {
        let backend = Rc::new(RawRingBackend::new(
            realm_id,
            clock_id,
            capacity_records,
            &[REALTIME_CATEGORY],
        )?);
        backend.set_enabled(false);
        RAW_BACKEND.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot.is_some() {
                return Err("a raw trace producer is already installed in this realm");
            }
            *slot = Some(Rc::clone(&backend));
            Ok(Self { backend })
        })
    }

    pub fn set_recording(&self, enabled: bool, engine_detail: bool) {
        self.backend.set_enabled(enabled);
        crate::set_engine_detail_enabled(engine_detail);
        crate::set_tracing_output_enabled(true);
        crate::set_tracing_enabled(enabled);
    }

    pub fn set_timestamp(&self, timestamp: u64) {
        self.backend.set_timestamp(timestamp);
    }

    pub fn available_records(&self) -> usize {
        self.backend.available_records()
    }

    pub fn drain_into(&self, destination: &mut [u8]) -> usize {
        self.backend.drain_into(destination)
    }

    pub fn health(&self) -> RawProducerHealth {
        let health = self.backend.health();
        RawProducerHealth {
            emitted_records: health.emitted_records,
            dropped_records: health.dropped_records,
            completed_drains: health.completed_drains,
            high_water_records: health.high_water_records,
        }
    }
}

impl Drop for RawTraceProducer {
    fn drop(&mut self) {
        self.set_recording(false, false);
        RAW_BACKEND.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot
                .as_ref()
                .is_some_and(|backend| Rc::ptr_eq(backend, &self.backend))
            {
                *slot = None;
            }
        });
    }
}

const fn static_name(label: &'static str) -> TraceMetadata {
    TraceMetadata {
        id: StaticName::new(label).id.0,
        namespace: 1,
        label,
    }
}

const fn category(label: &'static str) -> TraceMetadata {
    TraceMetadata {
        id: Category::new(label).id.0,
        namespace: 2,
        label,
    }
}

const fn field_name(label: &'static str) -> TraceMetadata {
    TraceMetadata {
        id: FieldName::new(label).id.0,
        namespace: 3,
        label,
    }
}

pub const REALTIME_METADATA: &[TraceMetadata] = &[
    category("shoop.realtime"),
    field_name("value"),
    static_name("span end"),
    static_name("engine.callback"),
    static_name("engine.fx.bridge.deadline_misses"),
    static_name("engine.fx.bridge.fallback_reason"),
    static_name("engine.fx.bridge.generation"),
    static_name("engine.fx.bridge.midi_input_overflows"),
    static_name("engine.fx.bridge.slot_occupancy"),
    static_name("engine.fx.global_midi.capacity_deferrals"),
    static_name("engine.fx.global_midi.pending_drained"),
    static_name("engine.fx.global_midi.pending_overwrites"),
    static_name("engine.fx.global_midi.rejected"),
    static_name("engine.meter.loop_output_peak_max_db"),
    static_name("engine.meter.track_input_peak_max_db"),
    static_name("engine.meter.track_output_peak_max_db"),
    static_name("engine.rt.callback"),
    static_name("engine.rt.channels.prepare"),
    static_name("engine.rt.channels.process"),
    static_name("engine.rt.commands"),
    static_name("engine.rt.composites.begin"),
    static_name("engine.rt.composites.timeline"),
    static_name("engine.rt.cycle"),
    static_name("engine.rt.driver"),
    static_name("engine.rt.driver.cpal_input"),
    static_name("engine.rt.driver.cpal_output"),
    static_name("engine.rt.driver.dummy"),
    static_name("engine.rt.driver.jack"),
    static_name("engine.rt.driver.midi_input"),
    static_name("engine.rt.fx.bridge_fallback"),
    static_name("engine.rt.fx.bridge_notify"),
    static_name("engine.rt.fx.bridge_process"),
    static_name("engine.rt.fx.bridge_submit"),
    static_name("engine.rt.fx.bridge_wait"),
    static_name("engine.rt.fx.bridge_worker"),
    static_name("engine.rt.fx.oxisynth_process"),
    static_name("engine.rt.fx.plugin_process"),
    static_name("engine.rt.fx.processor"),
    static_name("engine.rt.fx.subprocess_submit"),
    static_name("engine.rt.fx.subprocess_wait"),
    static_name("engine.rt.graph_state"),
    static_name("engine.rt.loops"),
    static_name("engine.rt.midi.playback"),
    static_name("engine.rt.ports.prepare"),
    static_name("engine.rt.ports.process"),
    static_name("engine.rt.publish_trace"),
    static_name("engine.rt.pump"),
    static_name("engine.rt.routing.external"),
    static_name("engine.rt.session"),
    static_name("engine.rt.state_publication"),
];
