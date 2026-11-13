//! Preparation and observation plumbing for an external runtime queue consumer.
use crate::cells::Cells;
use crate::*;
use alloc::sync::Arc;
use core::sync::atomic::Ordering;
use rt_runtime::{Frame, PreparedConfig, Quantum};

pub struct Preparation {
    pub config: PreparedConfig,
    pub quantum: Quantum,
    pub max_frames: usize,
    pub nodes: Vec<PreparedNode<EngineNode, LoopPlanner, EngineOp>>,
    pub planner: LoopPlanner,
    pub resources: ResourceStore,
    pub observations: Observations,
    pub loop_limits: LoopLimits,
    pub description: EngineDescription,
    pub(crate) recorder: Option<rt_runtime::profiling::Recorder>,
    pub(crate) midi_output: rt_runtime::buffer::MidiCapacity,
}
pub struct Consumer<R> {
    pub control: EngineControl,
    pub execution: R,
    pub latency_frames: usize,
}
pub struct Prepared<R> {
    pub endpoint: ControlEndpoint,
    pub execution: R,
    pub host: HostStatus,
}
impl<R> Prepared<R> {
    pub fn outbox(&self) -> shoop_rt_transport::carrier::Outbox {
        let limits = self.endpoint.limits();
        shoop_rt_transport::carrier::Outbox::new(
            2 * usize::from(limits.reliable_credits) + 8,
            limits.strips as usize + 2 * limits.loops as usize + limits.buses as usize + 2,
            usize::from(limits.bulk_credits),
        )
    }
}
impl Preparation {
    pub fn native(self) -> Result<Consumer<Render>, PrepareError> {
        let (control, mut runtime) = GraphRuntime::prepare_for_host(
            self.config,
            self.max_frames,
            self.quantum,
            self.nodes,
            self.planner,
            self.resources,
        )?;
        if let Some(recorder) = self.recorder {
            runtime.set_profiling(recorder);
        }
        if self.quantum.frames() > 1 {
            runtime.prepare_host_buffering_with_midi_output(self.midi_output)?;
        }
        runtime.prepare_graph_staging(1)?;
        let latency_frames = runtime.buffering_latency();
        let render = Render::new(
            runtime,
            self.observations.cells,
            [
                self.description.host.audio_inputs.len(),
                self.description.host.audio_outputs.len(),
                self.description.host.midi_inputs.len(),
                self.description.host.midi_outputs.len(),
            ],
            self.max_frames,
            self.description.resource_pages + 128,
        )?;
        Ok(Consumer {
            control,
            execution: render,
            latency_frames,
        })
    }
}

#[derive(Clone)]
pub struct Observations {
    pub(crate) cells: Arc<Cells>,
}
impl Observations {
    pub fn clock(&self, frame: rt_runtime::Frame, input_peak: f32, output_peak: f32) {
        self.cells.callbacks.fetch_add(1, Ordering::Relaxed);
        self.cells.next_frame.store(frame.0, Ordering::Relaxed);
        self.cells.host_peaks.raise(0, input_peak);
        self.cells.host_peaks.raise(1, output_peak);
    }
    pub fn fault(&self) {
        self.cells.faulted.store(true, Ordering::Relaxed);
    }
    pub fn connected(&self) -> bool {
        self.cells.endpoint_alive.load(Ordering::Relaxed)
    }
    pub fn loop_state(
        &self,
        frame: Frame,
        epoch: Epoch,
        state: shoop_loops::State,
        pending: Option<(u32, shoop_loops::Mode)>,
    ) {
        if let Some(row) = self.cells.loops.get(state.id.slot() as usize) {
            row.publish(frame, epoch, state, pending);
        }
    }
    pub fn loop_owner(&self, id: shoop_loops::LoopId, strip: u32) {
        if let Some(row) = self.cells.loops.get(id.slot() as usize) {
            row.configure_owner(
                id.generation(),
                strip,
                row.generation.load(Ordering::Relaxed),
            );
        }
    }

    pub fn strip_peaks(&self, slot: usize, input: &[f32], output: &[f32]) {
        if let Some(row) = self.cells.strips.get(slot) {
            for (ch, &peak) in input.iter().enumerate() {
                row.input.raise(ch, peak);
            }
            for (ch, &peak) in output.iter().enumerate() {
                row.output.raise(ch, peak);
            }
        }
    }
    pub fn bus_peaks(&self, slot: usize, peaks: &[f32]) {
        if let Some(row) = self.cells.buses.get(slot) {
            for (ch, &peak) in peaks.iter().enumerate() {
                row.raise(ch, peak);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rt_runtime::buffer::{AudioBlock, MidiBlock};
    use shoop_wasm_test_support::shoop_test;
    #[shoop_test]
    fn production_preparation_accepts_an_external_consumer_without_changing_native_rendering() {
        let description = EngineDescription {
            host: HostPorts::numbered(2, 2, 0),
            segments: vec![Segment {
                strips: 1,
                loops: 1,
            }],
            buses: 1,
            bus_channels: 2,
            ..Default::default()
        };
        let driver = DriverStatus {
            kind: shoop_rt_protocol::DriverKind::Dummy,
            state: shoop_rt_protocol::DriverRunState::Running,
            sample_rate: 48_000,
            buffer_frames: 512,
        };
        let mut called = false;
        let mut prepared = prepare_with_consumer(&description, driver, |parts| {
            called = true;
            assert_eq!(parts.quantum.frames(), 32);
            assert_eq!(parts.nodes.len(), 8);
            assert_eq!(parts.loop_limits.loop_slots, 1);
            parts.native()
        })
        .unwrap();
        assert!(called);
        assert_eq!(prepared.endpoint.limits().processing_latency_frames, 32);
        let mut left = [9.; 32];
        let mut right = [9.; 32];
        let report = prepared
            .execution
            .process(
                AudioBlock {
                    frame: Frame(0),
                    frames: 32,
                    inputs: &[&[1.; 32], &[0.; 32]],
                    outputs: &mut [&mut left, &mut right],
                },
                MidiBlock {
                    inputs: &[],
                    outputs: &mut [],
                },
            )
            .unwrap();
        assert_eq!(report.next_frame, Frame(32));
        assert_eq!(left, [0.; 32]);
        assert_eq!(right, [0.; 32]);
    }
}
