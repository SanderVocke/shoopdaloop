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

struct Echo(LocalTransport);

impl Backend for Echo {
    fn ports(&self) -> PortCounts {
        PORTS
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
                builtin_fx: ProcessorPolicy::On,
                oxisynth: ProcessorPolicy::Off,
                fx_instances: 1,
                synth_instances: 0,
                memory_mb: 16,
                carla: Default::default(),
            },
            ..EngineDescription::default()
        };
        let effective = description
            .processors
            .resolve(
                processors::Support {
                    builtin_fx: true,
                    oxisynth: false,
                },
                description.sample_rate,
            )
            .unwrap();
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
                LocalTransport::prepare(PORTS, timing, limits, timing.slots()).unwrap();
            prepared.reserved_bytes +=
                (LocalTransport::prepared_bytes(PORTS, timing, limits, timing.slots()).unwrap()
                    + limits
                        .async_storage_bytes_for(PORTS, timing.quantum(), TIMING)
                        .unwrap()) as u64;
            prepared.instances.push(PreparedProcessor {
                kind: ProcessorKind::BuiltInFx,
                backend: Box::new(Echo(endpoint)),
                timing: TIMING,
            });
            workers.push(worker);
        }
        let mut fixture = Self {
            host: OfflineHost::new_with_processors(&description, prepared).unwrap(),
            workers,
            fail_worker: false,
        };
        fixture.host.connect().unwrap();
        while fixture.host.receive().is_some() {}
        let strip = StripId(SlotId::new(0, 1));
        fixture.done(Request::CreateProcessedStrip {
            strip,
            shape: StripShape {
                audio_inputs: 2,
                audio_outputs: 2,
                midi_input: true,
                midi_output: false,
            },
            kind: ProcessorKind::BuiltInFx,
        });
        for (kind, index, host) in [
            (PortKind::AudioIn, 0, 0),
            (PortKind::AudioIn, 1, 1),
            (PortKind::MidiIn, 0, 4),
        ] {
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
        let frame = self.host.next_frame();
        assert_no_heap(|| {
            for worker in &mut self.workers {
                worker
                    .process_next(|_, batch| {
                        if self.fail_worker {
                            return Err(Error::Incomplete);
                        }
                        for channel in 0..2 {
                            let (input, output) = batch.audio.pair(channel, channel).unwrap();
                            for (output, input) in output.iter_mut().zip(input) {
                                *output = 2. * input;
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
        let id = self.host.request(request).unwrap();
        let outcome = self.wait_for(|event| match event {
            EngineMessage::Result {
                correlation,
                outcome,
            } if correlation == id => Some(outcome),
            _ => None,
        });
        assert_eq!(outcome, Outcome::Done, "{request:?}");
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
}
