use crate::{
    cells::LoopSnapshot, testing::OfflineHost, EngineDescription, HostPorts, PreparedProcessor,
    PreparedProcessors, Segment,
};
use alloc::{boxed::Box, vec, vec::Vec};
use rt_buffered::{
    AsyncTransport, Backend, Batch, BufferLimits, LocalTransport, LocalWorker, MidiLimits, Timing,
};
use rt_runtime::{graph::PortCounts, Error, Quantum};
use shoop_rt_protocol::*;
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

const TIMING: Timing = Timing {
    batch_quanta: 2,
    slack_quanta: 1,
};
const Q: usize = 16;
const PORTS: PortCounts = PortCounts {
    audio_inputs: 2,
    audio_outputs: 2,
    midi_inputs: 1,
    midi_outputs: 0,
};

struct Echo(LocalTransport, PortCounts);

impl Backend for Echo {
    fn ports(&self) -> PortCounts {
        self.1
    }

    fn async_transport(&mut self) -> Option<&mut dyn AsyncTransport> {
        Some(&mut self.0)
    }

    fn validate_parameter(&self, parameter: u32, value: f32) -> Result<(), Error> {
        if parameter == processors::WIDTH && value == 2. {
            Ok(())
        } else {
            Err(Error::InvalidParameter)
        }
    }

    fn control(&mut self, parameter: u32, value: f32) -> Result<(), Error> {
        self.validate_parameter(parameter, value)
    }

    fn process(&mut self, _: &mut Batch<'_, '_>) -> Result<(), Error> {
        panic!("worker called from the renderer")
    }

    fn suspend(&mut self) -> Result<(), Error> {
        Ok(())
    }

    fn reset(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

struct Fixture {
    host: OfflineHost,
    workers: Vec<LocalWorker>,
    fail_worker: bool,
    fail_source: Option<u64>,
    worker_gain: f32,
}

fn dry(frame: u64) -> f32 {
    (frame % 4096) as f32 / 4096.
}

fn packet(frame: u64) -> [u8; 3] {
    [0xb0, 1, ((frame / Q as u64) % 127) as u8]
}

fn target() -> LoopSlotId {
    LoopSlotId(SlotId::new(0, 1))
}

impl Fixture {
    fn new() -> Self {
        Self::for_processor(ProcessorKind::BuiltInFx, true)
    }

    fn for_processor(kind: ProcessorKind, dry_midi: bool) -> Self {
        let builtin = kind == ProcessorKind::BuiltInFx;
        let description = EngineDescription {
            quantum: Q,
            max_frames: Q,
            sample_rate: 8_000,
            ramp_frames: 0,
            control_lead_frames: 0,
            strip_channels: 2,
            host: HostPorts::numbered(2, 2, 1),
            segments: vec![Segment {
                strips: 1,
                loops: 1,
            }],
            processors: ProcessorSettings {
                builtin_fx: if builtin {
                    ProcessorPolicy::On
                } else {
                    ProcessorPolicy::Off
                },
                oxisynth: ProcessorPolicy::Off,
                fx_instances: 1,
                synth_instances: 0,
                memory_mb: 16,
                carla: processors::CarlaSettings {
                    hosting: if builtin {
                        processors::CarlaHosting::Disabled
                    } else {
                        processors::CarlaHosting::InProcess
                    },
                    patchbay_instances: if builtin { 0 } else { 2 },
                    ..Default::default()
                },
            },
            ..EngineDescription::default()
        };
        let mut effective = description
            .processors
            .resolve(
                processors::Support {
                    builtin_fx: true,
                    oxisynth: false,
                },
                description.sample_rate,
            )
            .unwrap();
        if kind.is_carla() {
            effective.carla.patchbay_instances = description.processors.carla.patchbay_instances;
            effective.carla.unavailable = None;
        }
        let ports = PortCounts {
            midi_outputs: usize::from(kind.is_carla()),
            ..PORTS
        };
        let timing = TIMING.prepare(Quantum::new(Q).unwrap()).unwrap();
        let capacity = description.processor_midi_input_capacity().unwrap();
        let limits = BufferLimits {
            midi_input: MidiLimits {
                events_per_quantum: capacity.events,
                bytes_per_quantum: capacity.bytes,
            },
            midi_output: MidiLimits::default(),
            parameters_per_quantum: 64,
        };
        let mut prepared = PreparedProcessors {
            effective,
            ..Default::default()
        };
        let mut workers = Vec::new();
        for _ in 0..effective.slots() {
            let (endpoint, worker) =
                LocalTransport::prepare(ports, timing, limits, timing.slots()).unwrap();
            prepared.reserved_bytes +=
                (LocalTransport::prepared_bytes(ports, timing, limits, timing.slots()).unwrap()
                    + limits
                        .async_storage_bytes_for(ports, timing.quantum(), TIMING)
                        .unwrap()) as u64;
            prepared.instances.push(PreparedProcessor {
                kind,
                backend: Box::new(Echo(endpoint, ports)),
                timing: TIMING,
            });
            workers.push(worker);
        }
        let mut fixture = Self {
            host: OfflineHost::new_with_processors(&description, prepared).unwrap(),
            workers,
            fail_worker: false,
            fail_source: None,
            worker_gain: 2.,
        };
        fixture.host.connect().unwrap();
        while fixture.host.receive().is_some() {}
        let strip = StripId(SlotId::new(0, 1));
        fixture.done(Request::CreateProcessedStrip {
            strip,
            shape: StripShape {
                audio_inputs: 2,
                audio_outputs: 2,
                midi_input: dry_midi,
                midi_output: false,
            },
            kind,
            remote_fx: None,
        });
        for (kind, index, host) in [
            (PortKind::AudioIn, 0, 0),
            (PortKind::AudioIn, 1, 1),
            (PortKind::MidiIn, 0, 4),
        ] {
            if kind == PortKind::MidiIn && !dry_midi {
                continue;
            }
            fixture.done(Request::Connect {
                port: EnginePort::Strip(StripPort { strip, kind, index }),
                host: HostPortId(host),
                connected: true,
            });
        }
        fixture.done(Request::CreateLoop {
            loop_id: target(),
            strip,
        });
        fixture
    }

    fn state(&mut self) -> LoopSnapshot {
        self.host.endpoint().cells.loops[0].snapshot().unwrap()
    }

    fn step(&mut self) {
        self.step_observing(|_, _| {});
    }

    fn step_observing(&mut self, mut observe: impl FnMut(usize, &Batch<'_, '_>)) {
        let frame = self.host.next_frame();
        assert_no_heap(|| {
            for (index, worker) in self.workers.iter_mut().enumerate() {
                worker
                    .process_next(|_, batch| {
                        observe(index, batch);
                        if self.fail_worker
                            || self.fail_source.is_some_and(|source| {
                                (batch.tag.source.0..batch.tag.source.0 + batch.frames as u64)
                                    .contains(&source)
                            })
                        {
                            return Err(Error::Incomplete);
                        }
                        for channel in 0..2 {
                            let (input, output) = batch.audio.pair(channel, channel).unwrap();
                            for (output, input) in output.iter_mut().zip(input) {
                                *output = self.worker_gain * input;
                            }
                        }
                        Ok(())
                    })
                    .unwrap();
            }
            for offset in 0..Q {
                let sample = dry(frame + offset as u64);
                self.host.input_mut(0)[offset] = sample;
                self.host.input_mut(1)[offset] = -sample;
            }
            self.host
                .midi_input_mut(0)
                .push(7, Q, &packet(frame))
                .unwrap();
            self.host.process(Q).unwrap();
        });
    }

    fn done(&mut self, request: Request) {
        assert_eq!(self.answer(request), Outcome::Done, "{request:?}");
    }

    fn answer(&mut self, request: Request) -> Outcome {
        let id = self.host.request(request).unwrap();
        self.wait_for(|event| match event {
            EngineMessage::Result {
                correlation,
                outcome,
            } if correlation == id => Some(outcome),
            _ => None,
        })
    }

    fn replacing(&mut self, id: u64, position: u64) -> u64 {
        for request in [
            Request::InputBegin { id, count: 1 },
            Request::InputAppend {
                id,
                edit: InputEdit {
                    loop_id: target(),
                    intent: InputIntent::Mode {
                        mode: LoopMode::Replacing,
                        position: Some(position),
                    },
                },
            },
            Request::InputPrepare { id },
        ] {
            assert!(
                matches!(self.answer(request), Outcome::Input(_)),
                "{request:?}"
            );
        }
        let Outcome::Input(progress) = self.answer(Request::InputCommit { id, delay: None }) else {
            panic!("replace input")
        };
        assert_eq!(progress.phase, InputPhase::Applied);
        progress.effective_frame.unwrap()
    }

    fn wait_for<T>(&mut self, mut find: impl FnMut(EngineMessage) -> Option<T>) -> T {
        for _ in 0..512 {
            while let Some(event) = self.host.receive() {
                if let Some(value) = find(event) {
                    return value;
                }
            }
            self.step();
        }
        panic!("engine response timed out")
    }

    fn mode(&mut self, mode: LoopMode) -> LoopSnapshot {
        self.host
            .request(Request::LoopTransition {
                loop_id: target(),
                mode,
                delay: None,
            })
            .unwrap();
        for _ in 0..128 {
            while self.host.receive().is_some() {}
            self.step();
            let state = self.state();
            if crate::endpoint::protocol_mode(state.mode) == mode {
                return state;
            }
        }
        panic!("mode {mode:?} timed out")
    }

    fn read(&mut self, selector: BulkSelector, id: u32, offset: u64) -> BulkChunk {
        self.host
            .bulk(BulkRead {
                id,
                loop_id: target(),
                selector,
                offset,
            })
            .unwrap();
        self.wait_for(|event| match event {
            EngineMessage::Bulk(chunk) if chunk.id == id => Some(chunk),
            _ => None,
        })
    }
}

#[shoop_test]
fn global_controls_reach_carla_track_without_dry_midi_and_bus_once() {
    let kind = ProcessorKind::CarlaPatchbay;
    let mut fixture = Fixture::for_processor(kind, false);
    fixture.done(Request::CreateProcessedBus {
        bus: BusId(SlotId::new(1, 1)),
        channels: 2,
        kind,
    });
    fixture.done(Request::Connect {
        port: EnginePort::GlobalFxControl,
        host: HostPortId(4),
        connected: true,
    });
    fixture.done(Request::StripControl {
        strip: StripId(SlotId::new(0, 1)),
        control: StripControl::Monitoring(true),
    });
    for _ in 0..8 {
        fixture.step();
    }
    let frame = fixture.host.next_frame();
    let control = [0xb0, 99, 87];
    fixture.host.midi_input_mut(0).push(3, Q, &control).unwrap();
    let mut received = [0; 4];
    for _ in 0..8 {
        fixture.step_observing(|index, batch| {
            for (offset, data) in batch.midi_input(0).unwrap().iter() {
                if data == control {
                    assert_eq!(batch.tag.source.0 + u64::from(offset), frame + 3);
                    received[index] += 1;
                }
            }
        });
    }
    received.sort_unstable();
    assert_eq!(received, [0, 0, 1, 1]);
}

#[shoop_test]
fn worker_capture_drains_at_stop_and_keeps_committed_roles_after_a_lost_tail() {
    let mut fixture = Fixture::new();
    let start = fixture.mode(LoopMode::Recording).frame;
    fixture.step();
    fixture.step();
    let cut = fixture.mode(LoopMode::Stopped);
    assert_eq!(cut.revision, 0);
    assert_eq!(cut.length, 0);
    let frames = cut.frame - start;
    assert!(frames >= 3 * Q as u64);
    let publish = cut.frame + ((TIMING.batch_quanta + TIMING.slack_quanta) * Q) as u64;
    while fixture.host.next_frame() < publish {
        fixture.step();
        assert_eq!(fixture.state().revision, 0);
    }
    fixture.step();
    assert_eq!(fixture.state().revision, 1);
    assert_eq!(fixture.state().length, frames);

    let mut offset = 0;
    for id in 1..16 {
        let chunk = fixture.read(BulkSelector::AudioRoles, id, offset);
        assert_eq!(chunk.offset, offset);
        assert_eq!(chunk.total, frames);
        let BulkPayload::Audio { len, samples } = chunk.payload else {
            panic!("{chunk:?}");
        };
        for values in samples[..usize::from(len)].chunks_exact(4) {
            let sample = dry(start + offset);
            assert_eq!(values, [sample, -sample, 2. * sample, -2. * sample]);
            offset += 1;
        }
        if offset == frames {
            break;
        }
    }
    assert_eq!(offset, frames);

    let chunk = fixture.read(BulkSelector::Midi, 16, 0);
    let BulkPayload::Midi { len, bytes } = chunk.payload else {
        panic!("{chunk:?}");
    };
    assert_eq!(u64::from(len), frames / Q as u64 * 15);
    for (tick, event) in bytes[..usize::from(len)].chunks_exact(15).enumerate() {
        let offset = (tick * Q) as u64;
        assert_eq!(event[..8], (offset + 7).to_le_bytes());
        assert_eq!(event[8..12], 3_u32.to_le_bytes());
        assert_eq!(event[12..], packet(start + offset));
    }

    fixture.mode(LoopMode::Recording);
    let stop = fixture.mode(LoopMode::Stopped);
    assert_eq!(stop.revision, 1);
    assert_eq!(stop.length, frames);
    fixture.fail_worker = true;
    for _ in 0..6 {
        fixture.step();
    }
    let state = fixture.state();
    assert!(state.capture_incomplete);
    assert_eq!(state.revision, 1);
    assert_eq!(state.length, frames);
    let chunk = fixture.read(BulkSelector::AudioRoles, 17, 0);
    let BulkPayload::Audio { samples, .. } = chunk.payload else {
        panic!("{chunk:?}");
    };
    assert_eq!(
        &samples[..4],
        [dry(start), -dry(start), 2. * dry(start), -2. * dry(start)]
    );
    fixture.done(Request::StripControl {
        strip: StripId(SlotId::new(0, 1)),
        control: StripControl::Monitoring(true),
    });
    let diagnostics = fixture.wait_for(|event| match event {
        EngineMessage::State(StateRecord::ProcessorState { snapshot, .. }) => snapshot
            .worker
            .filter(|worker| worker.failed > 0 && worker.missed > 0),
        _ => None,
    });
    assert!(diagnostics.peak_pending > 0);
    assert!(diagnostics.pending <= diagnostics.peak_pending);
}

#[shoop_test]
fn worker_rerecords_wet_at_wrapped_source_positions_and_rolls_back_a_lost_tail() {
    let mut fixture = Fixture::new();
    let start = fixture.mode(LoopMode::Recording).frame;
    fixture.step();
    fixture.step();
    let cut = fixture.mode(LoopMode::Stopped).frame;
    let frames = cut - start;
    let delay_quanta = TIMING.batch_quanta + TIMING.slack_quanta;
    for _ in 0..=delay_quanta {
        fixture.step();
    }
    assert_eq!(fixture.state().revision, 1);
    let midi = fixture.read(BulkSelector::Midi, 1, 0).payload;

    fixture.worker_gain = 3.;
    fixture.mode(LoopMode::RecordingDryIntoWet);
    for _ in 0..frames / Q as u64 + 2 {
        fixture.step();
    }
    let stopped = fixture.mode(LoopMode::Stopped);
    assert_eq!(stopped.revision, 1);
    let publish = stopped.frame + (delay_quanta * Q) as u64;
    while fixture.host.next_frame() < publish {
        fixture.step();
        assert_eq!(fixture.state().revision, 1);
    }
    fixture.step();
    assert_eq!(fixture.state().revision, 2);
    let rerecorded = fixture.read(BulkSelector::AudioRoles, 2, 0);
    assert_eq!(rerecorded.total, frames);
    let BulkPayload::Audio { len, samples } = rerecorded.payload else {
        panic!("{rerecorded:?}");
    };
    assert_eq!(u64::from(len), 4 * frames);
    for (offset, values) in samples[..usize::from(len)].chunks_exact(4).enumerate() {
        let sample = dry(start + offset as u64);
        assert_eq!(values, [sample, -sample, 3. * sample, -3. * sample]);
    }
    assert_eq!(fixture.read(BulkSelector::Midi, 3, 0).payload, midi);

    fixture.worker_gain = 4.;
    fixture.mode(LoopMode::RecordingDryIntoWet);
    for _ in 0..=delay_quanta {
        fixture.step();
    }
    assert_eq!(fixture.mode(LoopMode::Stopped).revision, 2);
    fixture.fail_worker = true;
    for _ in 0..=delay_quanta + 2 {
        fixture.step();
    }
    assert!(fixture.state().capture_incomplete);
    assert_eq!(fixture.state().revision, 2);
    assert_eq!(
        fixture.read(BulkSelector::AudioRoles, 4, 0).payload,
        rerecorded.payload
    );
    assert_eq!(fixture.read(BulkSelector::Midi, 5, 0).payload, midi);
}

#[shoop_test]
fn worker_replacing_pairs_wrapped_dry_wet_and_midi_and_rolls_back_a_missing_tail() {
    let mut fixture = Fixture::new();
    let start = fixture.mode(LoopMode::Recording).frame;
    fixture.step();
    fixture.step();
    let length = fixture.mode(LoopMode::Stopped).frame - start;
    let delay = (TIMING.batch_quanta + TIMING.slack_quanta) * Q;
    for _ in 0..=delay / Q {
        fixture.step();
    }
    assert_eq!(fixture.state().revision, 1);

    fixture.worker_gain = 3.;
    let position = Q as u64;
    let start = fixture.replacing(1, position);
    for _ in 0..length / Q as u64 + 2 {
        fixture.step();
    }
    let cut = fixture.mode(LoopMode::Stopped);
    assert_eq!(cut.revision, 1);
    let frames = cut.frame - start;
    assert!(frames > length);
    while fixture.host.next_frame() < cut.frame + delay as u64 {
        fixture.step();
        assert_eq!(fixture.state().revision, 1);
    }
    fixture.step();
    assert_eq!(fixture.state().revision, 2);
    assert_eq!(fixture.state().length, length);
    let source = |offset: u64| {
        let first = (offset + length - position) % length;
        start + first + (frames - first - 1) / length * length
    };
    let audio = fixture.read(BulkSelector::AudioRoles, 1, 0);
    assert_eq!(audio.total, length);
    let BulkPayload::Audio { len, samples } = audio.payload else {
        panic!("audio")
    };
    assert_eq!(u64::from(len), 4 * length);
    for (offset, actual) in samples[..usize::from(len)].chunks_exact(4).enumerate() {
        let sample = dry(source(offset as u64));
        assert_eq!(actual, [sample, -sample, 3. * sample, -3. * sample]);
    }
    let midi = fixture.read(BulkSelector::Midi, 2, 0).payload;
    let BulkPayload::Midi { len, bytes } = midi else {
        panic!("MIDI")
    };
    assert_eq!(u64::from(len), length / Q as u64 * 15);
    for (tick, event) in bytes[..usize::from(len)].chunks_exact(15).enumerate() {
        let offset = (tick * Q) as u64;
        assert_eq!(event[..8], (offset + 7).to_le_bytes());
        assert_eq!(event[8..12], 3_u32.to_le_bytes());
        assert_eq!(event[12..], packet(source(offset)));
    }

    fixture.worker_gain = 4.;
    fixture.replacing(2, 0);
    for _ in 0..=delay / Q {
        fixture.step();
    }
    let missing = fixture.host.next_frame();
    fixture.fail_source = Some(missing);
    fixture.step();
    fixture.step();
    let stopped = fixture.mode(LoopMode::Stopped);
    assert_eq!(stopped.revision, 2);
    assert!(stopped.frame > missing);
    for _ in 0..=delay / Q + 2 {
        fixture.step();
    }
    assert!(fixture.state().capture_incomplete);
    assert_eq!(fixture.state().revision, 2);
    assert_eq!(
        fixture.read(BulkSelector::AudioRoles, 3, 0).payload,
        audio.payload
    );
    assert_eq!(fixture.read(BulkSelector::Midi, 4, 0).payload, midi);
}
