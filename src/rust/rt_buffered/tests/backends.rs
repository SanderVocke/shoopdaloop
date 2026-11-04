use rt_buffered::{Backend, Batch, BufferLimits, BufferedProcessor, Lifecycle, MidiLimits, Timing};
use rt_runtime::{
    buffer::{AudioPorts, MidiBuffer},
    graph::PortCounts,
    Error, Frame, Quantum,
};
use shoop_wasm_test_support::{
    heap::{assert_no_heap, TrackingAllocator},
    shoop_test,
};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

struct Gain {
    ports: PortCounts,
    value: f32,
    fail: bool,
}
impl Backend for Gain {
    fn ports(&self) -> PortCounts {
        self.ports
    }
    fn validate_parameter(&self, parameter: u32, value: f32) -> Result<(), Error> {
        if parameter == 0 && value.is_finite() && value >= 0. {
            Ok(())
        } else {
            Err(Error::InvalidParameter)
        }
    }
    fn reset(&mut self) -> Result<(), Error> {
        self.value = 0.5;
        Ok(())
    }
    fn process(&mut self, batch: &mut Batch<'_>) -> Result<(), Error> {
        if self.fail {
            return Err(Error::InvalidState);
        }
        let mut event = 0;
        for frame in 0..batch.frames {
            while let Some(parameter) = batch.parameters.get(event) {
                if parameter.offset as usize > frame {
                    break;
                }
                self.value = parameter.value;
                event += 1;
            }
            for channel in 0..self.ports.audio_outputs {
                let sample = if self.ports.audio_inputs == 0 {
                    channel as f32 + 1.
                } else {
                    batch
                        .audio
                        .input(channel % self.ports.audio_inputs)
                        .unwrap()[frame]
                };
                batch.audio.output(channel).unwrap()[frame] = sample * self.value;
            }
        }
        for (port, output) in batch.midi_outputs.iter_mut().enumerate() {
            if let Some(input) = batch.midi_inputs.get(port) {
                for i in 0..input.len() {
                    let (at, bytes) = input.get(i).unwrap();
                    output.push(at, batch.frames, bytes)?;
                }
            }
        }
        Ok(())
    }
}
fn gain(ports: PortCounts, q: usize, m: usize, s: usize) -> BufferedProcessor<Gain> {
    BufferedProcessor::with_backend(
        Gain {
            ports,
            value: 0.5,
            fail: false,
        },
        Quantum::new(q).unwrap(),
        Timing {
            batch_quanta: m,
            slack_quanta: s,
        },
        BufferLimits::default(),
    )
    .unwrap()
}
fn render<B: Backend>(
    node: &mut BufferedProcessor<B>,
    frame: usize,
    input: &[f32],
    output: &mut [f32],
    midi_in: &[MidiBuffer],
    midi_out: &mut [MidiBuffer],
) -> Result<(), Error> {
    let q = node.timing().quantum();
    node.process_ports_tick(
        Frame(frame as u64),
        q,
        &mut AudioPorts::new(input, output, q.frames(), 0, q.frames()).unwrap(),
        midi_in,
        midi_out,
    )
}

#[shoop_test]
fn production_shapes_preserve_source_controls_audio_and_each_midi_lane_without_heap_work() {
    for (ai, ao, mi, mo) in [
        (0, 0, 0, 0),
        (0, 2, 1, 0),
        (1, 1, 0, 0),
        (3, 3, 1, 1),
        (8, 8, 1, 0),
        (2, 1, 2, 2),
    ] {
        let ports = PortCounts {
            audio_inputs: ai,
            audio_outputs: ao,
            midi_inputs: mi,
            midi_outputs: mo,
        };
        for q in [1, 16, 32, 64, 128] {
            for (m, s) in [(1, 0), (3, 0), (3, 7)] {
                let mut node = gain(ports, q, m, s);
                let mut input = vec![0.; ai * q];
                let mut output = vec![0.; ao * q];
                let mut midi_in: Vec<_> =
                    (0..mi).map(|_| MidiBuffer::new(4, 12).unwrap()).collect();
                let mut midi_out: Vec<_> =
                    (0..mo).map(|_| MidiBuffer::new(4, 12).unwrap()).collect();
                let delay = (m + s) * q;
                for tick in 0..m + s + 12 * m {
                    let frame = tick * q;
                    for c in 0..ai {
                        input[c * q..(c + 1) * q].fill(c as f32 + 1.);
                    }
                    for (port, buffer) in midi_in.iter_mut().enumerate() {
                        buffer.clear();
                        buffer
                            .push(
                                (q - 1) as u32,
                                q,
                                &[0x90 + port as u8, 60 + (tick % 12) as u8, 100],
                            )
                            .unwrap();
                    }
                    for buffer in &mut midi_out {
                        buffer.clear();
                    }
                    assert_no_heap(|| {
                        node.schedule_parameter(Frame((frame + q - 1) as u64), 0, 9.)
                            .unwrap();
                        node.schedule_parameter(
                            Frame((frame + q - 1) as u64),
                            0,
                            (tick % 3 + 1) as f32,
                        )
                        .unwrap();
                        render(
                            &mut node,
                            frame,
                            &input,
                            &mut output,
                            &midi_in,
                            &mut midi_out,
                        )
                        .unwrap();
                    });
                    for c in 0..ao {
                        for f in 0..q {
                            let at = frame + f;
                            let expected = if at < delay {
                                0.
                            } else {
                                let source = at - delay;
                                let source_tick = source / q;
                                let value = if source % q == q - 1 {
                                    (source_tick % 3 + 1) as f32
                                } else if source_tick == 0 {
                                    0.5
                                } else {
                                    ((source_tick - 1) % 3 + 1) as f32
                                };
                                let sample = if ai == 0 {
                                    c as f32 + 1.
                                } else {
                                    (c % ai) as f32 + 1.
                                };
                                sample * value
                            };
                            assert_eq!(
                                output[c * q + f],
                                expected,
                                "shape={ports:?}, q={q},m={m},s={s},frame={at}"
                            );
                        }
                    }
                    for (port, buffer) in midi_out.iter().enumerate() {
                        if frame < delay {
                            assert!(buffer.is_empty());
                        } else {
                            let source_tick = (frame - delay) / q;
                            assert_eq!(buffer.len(), 1);
                            assert_eq!(
                                buffer.get(0),
                                Some((
                                    (q - 1) as u32,
                                    &[0x90 + port as u8, 60 + (source_tick % 12) as u8, 100][..]
                                ))
                            );
                        }
                    }
                }
            }
        }
    }
}

#[shoop_test]
fn partial_drain_pads_once_and_suspends_without_accepting_new_live_input() {
    let ports = PortCounts {
        audio_inputs: 1,
        audio_outputs: 1,
        ..PortCounts::default()
    };
    let mut node = gain(ports, 16, 3, 0);
    let mut output = [0.; 16];
    for frame in [0, 16] {
        render(&mut node, frame, &[2.; 16], &mut output, &[], &mut []).unwrap();
    }
    node.begin_drain().unwrap();
    for frame in [32, 48, 64, 80, 96, 112] {
        assert_no_heap(|| {
            render(&mut node, frame, &[100.; 16], &mut output, &[], &mut []).unwrap()
        });
        assert_eq!(
            output,
            if frame == 48 || frame == 64 {
                [1.; 16]
            } else {
                [0.; 16]
            }
        );
    }
    assert_eq!(node.lifecycle(), Lifecycle::Suspended);
    assert_eq!(node.completed_batches(), 1);
    assert_eq!(
        node.schedule_parameter(Frame(128), 0, 1.),
        Err(Error::InvalidState)
    );
    assert_no_heap(|| node.activate(Frame(256)).unwrap());
    render(&mut node, 256, &[4.; 16], &mut output, &[], &mut []).unwrap();
    assert_eq!(output, [0.; 16]);
}

#[shoop_test]
fn abort_clears_partial_controls_and_midi_in_every_old_slot_generation() {
    let ports = PortCounts {
        audio_inputs: 1,
        audio_outputs: 1,
        midi_inputs: 1,
        midi_outputs: 1,
    };
    let mut node = gain(ports, 16, 3, 7);
    let mut input = MidiBuffer::new(8, 24).unwrap();
    let mut output = MidiBuffer::new(8, 24).unwrap();
    let mut audio = [0.; 16];
    for frame in (0..80).step_by(16) {
        input.clear();
        output.clear();
        input.push(0, 16, &[0x90, 60, 100]).unwrap();
        node.schedule_parameter(Frame(frame as u64), 0, 99.)
            .unwrap();
        render(
            &mut node,
            frame,
            &[2.; 16],
            &mut audio,
            core::slice::from_ref(&input),
            core::slice::from_mut(&mut output),
        )
        .unwrap();
    }
    assert_no_heap(|| {
        node.abort(Frame(512)).unwrap();
        node.activate(Frame(512)).unwrap();
    });
    input.clear();
    for frame in (512..1056).step_by(16) {
        output.clear();
        assert_no_heap(|| {
            render(
                &mut node,
                frame,
                &[2.; 16],
                &mut audio,
                core::slice::from_ref(&input),
                core::slice::from_mut(&mut output),
            )
            .unwrap()
        });
        assert!(output.is_empty());
        assert_eq!(audio, if frame < 672 { [0.; 16] } else { [1.; 16] });
    }
}

#[shoop_test]
fn parameter_admission_and_backend_faults_require_explicit_recovery() {
    let ports = PortCounts {
        audio_inputs: 1,
        audio_outputs: 1,
        ..PortCounts::default()
    };
    let mut node = gain(ports, 16, 1, 0);
    assert_eq!(
        node.schedule_parameter(Frame(0), 1, 1.),
        Err(Error::InvalidParameter)
    );
    assert_eq!(
        node.schedule_parameter(Frame(0), 0, f32::NAN),
        Err(Error::InvalidParameter)
    );
    assert_eq!(
        node.schedule_parameter(Frame(16), 0, 1.),
        Err(Error::InvalidFrame)
    );
    node.schedule_parameter(Frame(15), 0, 2.).unwrap();
    assert_eq!(
        node.schedule_parameter(Frame(14), 0, 1.),
        Err(Error::InvalidFrame)
    );
    for _ in 1..64 {
        node.schedule_parameter(Frame(15), 0, 2.).unwrap();
    }
    assert_eq!(
        node.schedule_parameter(Frame(15), 0, 2.),
        Err(Error::Capacity)
    );
    node.backend();
    let mut broken = BufferedProcessor::with_backend(
        Gain {
            ports,
            value: 1.,
            fail: true,
        },
        Quantum::new(16).unwrap(),
        Timing {
            batch_quanta: 1,
            slack_quanta: 0,
        },
        BufferLimits::default(),
    )
    .unwrap();
    let mut out = [0.; 16];
    assert_eq!(
        render(&mut broken, 0, &[2.; 16], &mut out, &[], &mut []),
        Err(Error::InvalidState)
    );
    assert_eq!(broken.lifecycle(), Lifecycle::Faulted);
    assert_eq!(
        render(&mut broken, 0, &[2.; 16], &mut out, &[], &mut []),
        Err(Error::InvalidState)
    );
}

#[shoop_test]
fn storage_overflow_is_refused_before_any_adapter_allocation() {
    assert_no_heap(|| {
        assert!(matches!(
            BufferedProcessor::prepare(
                Quantum::new(16).unwrap(),
                Timing::default(),
                MidiLimits {
                    events_per_quantum: 1,
                    bytes_per_quantum: usize::MAX
                }
            ),
            Err(Error::Capacity)
        ));
        let ports = PortCounts {
            audio_inputs: usize::MAX,
            ..PortCounts::default()
        };
        assert!(matches!(
            BufferedProcessor::with_backend(
                Gain {
                    ports,
                    value: 1.,
                    fail: false
                },
                Quantum::new(16).unwrap(),
                Timing::default(),
                BufferLimits::default()
            ),
            Err(Error::Capacity)
        ));
    });
}

struct Delay {
    history: [f32; 3],
    cursor: usize,
}
impl Backend for Delay {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 1,
            audio_outputs: 1,
            ..PortCounts::default()
        }
    }
    fn latency_frames(&self) -> usize {
        3
    }
    fn reset(&mut self) -> Result<(), Error> {
        self.history.fill(0.);
        self.cursor = 0;
        Ok(())
    }
    fn process(&mut self, batch: &mut Batch<'_>) -> Result<(), Error> {
        for frame in 0..batch.frames {
            let input = batch.audio.input(0).unwrap()[frame];
            batch.audio.output(0).unwrap()[frame] = self.history[self.cursor];
            self.history[self.cursor] = input;
            self.cursor = (self.cursor + 1) % 3;
        }
        Ok(())
    }
}
#[shoop_test]
fn backend_latency_is_reported_but_not_added_twice_to_release_time() {
    let mut node = BufferedProcessor::with_backend(
        Delay {
            history: [0.; 3],
            cursor: 0,
        },
        Quantum::new(16).unwrap(),
        Timing {
            batch_quanta: 1,
            slack_quanta: 0,
        },
        BufferLimits::default(),
    )
    .unwrap();
    assert_eq!(node.latency_frames(), 19);
    let mut input = [0.; 16];
    input[0] = 1.;
    let mut output = [0.; 16];
    render(&mut node, 0, &input, &mut output, &[], &mut []).unwrap();
    assert_eq!(output, [0.; 16]);
    render(&mut node, 16, &[0.; 16], &mut output, &[], &mut []).unwrap();
    assert_eq!(output.iter().position(|v| *v == 1.), Some(3));
    assert_eq!(output.iter().sum::<f32>(), 1.);
}
