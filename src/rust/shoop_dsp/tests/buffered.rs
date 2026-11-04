#![cfg(any(feature = "builtin-fx", feature = "oxisynth"))]
use rt_buffered::{Backend, BufferLimits, BufferedProcessor, Timing};
use rt_runtime::{
    buffer::{AudioPorts, MidiBuffer},
    Frame, Quantum,
};
use shoop_wasm_test_support::{
    heap::{assert_no_heap, TrackingAllocator},
    shoop_test,
};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

struct Harness<B> {
    node: BufferedProcessor<B>,
    input: Vec<f32>,
    output: Vec<f32>,
    midi: MidiBuffer,
    frame: usize,
}
impl<B: Backend> Harness<B> {
    fn new(backend: B, q: usize) -> Self {
        let ports = backend.ports();
        Self {
            node: BufferedProcessor::with_backend(
                backend,
                Quantum::new(q).unwrap(),
                Timing {
                    batch_quanta: 1,
                    slack_quanta: 0,
                },
                BufferLimits {
                    parameters_per_quantum: 128,
                    ..BufferLimits::default()
                },
            )
            .unwrap(),
            input: vec![0.; ports.audio_inputs * q],
            output: vec![0.; ports.audio_outputs * q],
            midi: MidiBuffer::new(64, 4096).unwrap(),
            frame: 0,
        }
    }
    fn control(&mut self, id: u32, value: f32) {
        self.node
            .schedule_parameter(Frame(self.frame as u64), id, value)
            .unwrap();
    }
    fn render(&mut self) {
        let q = self.node.timing().quantum();
        self.node
            .process_ports_tick(
                Frame(self.frame as u64),
                q,
                &mut AudioPorts::new(&self.input, &mut self.output, q.frames(), 0, q.frames())
                    .unwrap(),
                core::slice::from_ref(&self.midi),
                &mut [],
            )
            .unwrap();
        self.frame += q.frames();
        self.midi.clear();
    }
}

#[cfg(feature = "builtin-fx")]
#[shoop_test]
fn prepared_fx_slot_preserves_reference_behavior_at_every_active_width() {
    use shoop_dsp::{
        backends::{FxBackend, WIDTH},
        builtin_fx::{BuiltInFxProcessor, BuiltInFxState},
    };
    for channels in 1..=8 {
        let mut h = Harness::new(FxBackend::prepare(48000., 64, 8).unwrap(), 64);
        let mut reference =
            BuiltInFxProcessor::new_with_channels(48000., 64, channels, BuiltInFxState::default())
                .unwrap();
        h.control(WIDTH, channels as f32);
        let mut previous = vec![0.; channels * 64];
        for tick in 0..8 {
            for c in 0..8 {
                for f in 0..64 {
                    h.input[c * 64 + f] = if tick == 0 && f == c { 1. } else { 0. };
                }
                if c < channels {
                    reference
                        .input_mut(c, 64)
                        .unwrap()
                        .copy_from_slice(&h.input[c * 64..c * 64 + 64]);
                }
            }
            assert_no_heap(|| h.render());
            for c in 0..8 {
                if c < channels {
                    assert_eq!(
                        &h.output[c * 64..c * 64 + 64],
                        &previous[c * 64..c * 64 + 64],
                        "channels={channels},tick={tick},c={c}"
                    );
                } else {
                    assert!(h.output[c * 64..c * 64 + 64].iter().all(|v| *v == 0.));
                }
            }
            reference.process(64);
            for c in 0..channels {
                previous[c * 64..c * 64 + 64].copy_from_slice(reference.output(c, 64).unwrap());
            }
        }
    }
}

#[cfg(feature = "builtin-fx")]
#[shoop_test]
fn fx_midi_mapping_state_and_activation_reset_are_applied_without_heap_work() {
    use shoop_dsp::{
        backends::{FxBackend, CC_BASE, STAGE_BASE},
        builtin_fx::{BuiltInFxParameter, BuiltInFxState},
    };
    let mut h = Harness::new(FxBackend::prepare(48000., 32, 2).unwrap(), 32);
    h.control(CC_BASE + 5, 13.);
    for stage in 0..6 {
        h.control(STAGE_BASE + stage, 0.);
    }
    h.midi.push(31, 32, &[0xb0, 13, 127]).unwrap();
    h.input.fill(0.25);
    assert_no_heap(|| h.render());
    assert_eq!(
        h.node.backend().parameter(5),
        Some(BuiltInFxParameter::Drive.value_from_cc(127))
    );
    assert_eq!(h.node.backend().parameter(CC_BASE + 5), Some(13.));
    assert_no_heap(|| h.render());
    assert_eq!(h.output, vec![0.25; 64]);
    assert_no_heap(|| {
        h.node.abort(Frame(128)).unwrap();
        h.node.activate(Frame(128)).unwrap();
    });
    h.frame = 128;
    assert_eq!(h.node.backend().state(), BuiltInFxState::default());
    assert_eq!(h.node.backend().parameter(CC_BASE + 5), Some(-1.));
    h.input.fill(0.);
    for _ in 0..5 {
        assert_no_heap(|| h.render());
        assert!(h.output.iter().all(|v| v.abs() < 1e-6));
    }
}

#[cfg(feature = "oxisynth")]
#[shoop_test]
fn cold_synth_all_presets_voice_growth_stealing_and_reset_do_not_touch_heap() {
    use shoop_dsp::{
        backends::{SynthBackend, SYNTH_PANIC, SYNTH_PRESET},
        oxisynth::available_presets,
    };
    let mut h = Harness::new(SynthBackend::prepare(48000., 64).unwrap(), 64);
    for preset in available_presets() {
        h.control(
            SYNTH_PRESET,
            (u32::from(preset.id.bank) * 128 + u32::from(preset.id.program)) as f32,
        );
        for key in 24..88 {
            h.midi.push(0, 64, &[0x90, key, 100]).unwrap();
        }
        assert_no_heap(|| h.render());
        for round in 0..6 {
            for key in 24..88 {
                h.midi.push(round, 64, &[0x90, key, 100]).unwrap();
            }
            assert_no_heap(|| h.render());
        }
        h.control(SYNTH_PANIC, 0.);
        assert_no_heap(|| h.render());
        assert!(h.output.iter().all(|v| v.is_finite()));
    }
    let origin = Frame(h.frame as u64);
    assert_no_heap(|| {
        h.node.abort(origin).unwrap();
        h.node.activate(origin).unwrap();
    });
    h.input.fill(0.);
    for _ in 0..16 {
        assert_no_heap(|| h.render());
        assert!(h.output.iter().all(|v| v.abs() < 1e-6));
    }
}

#[cfg(feature = "oxisynth")]
#[shoop_test]
fn buffered_synth_retains_reference_event_offsets_and_ignores_dry_audio() {
    use shoop_dsp::{
        backends::{SynthBackend, CC_BASE},
        midi_storage::MidiStorageElem,
        oxisynth::{OxiSynthProcessor, OxiSynthState},
    };
    let mut h = Harness::new(SynthBackend::prepare(48000., 128).unwrap(), 128);
    let mut reference = OxiSynthProcessor::new(48000., 128, OxiSynthState::default()).unwrap();
    let mut expected = vec![0.; 256];
    for tick in 0..16 {
        h.input.fill(if tick % 2 == 0 { 100. } else { -100. });
        let note = MidiStorageElem::new(63, &[0x90, 60, 100]).unwrap();
        if tick == 0 {
            h.midi.push(63, 128, note.data()).unwrap();
        }
        assert_no_heap(|| h.render());
        assert_eq!(h.output, expected);
        reference.process(
            128,
            if tick == 0 {
                core::slice::from_ref(&note)
            } else {
                &[]
            },
        );
        for c in 0..2 {
            expected[c * 128..c * 128 + 128].copy_from_slice(reference.output(c, 128).unwrap());
        }
    }
    h.control(CC_BASE, 7.);
    h.midi.push(17, 128, &[0xb0, 7, 127]).unwrap();
    assert_no_heap(|| h.render());
    assert_eq!(h.node.backend().parameter(0), Some(1.));
    assert_eq!(h.node.backend().parameter(CC_BASE), Some(7.));
}

#[shoop_test]
fn prepared_backends_reject_unsupported_rates_and_shapes_before_preparation() {
    #[cfg(feature = "builtin-fx")]
    {
        use shoop_dsp::backends::FxBackend;
        for rate in [0., f32::NAN, f32::INFINITY, 384001.] {
            assert!(FxBackend::prepare(rate, 64, 2).is_err());
        }
        assert!(FxBackend::prepare(48000., 64, 0).is_err());
        assert!(FxBackend::prepare(48000., 64, 9).is_err());
        assert!(FxBackend::prepare(48000., 0, 2).is_err());
    }
    #[cfg(feature = "oxisynth")]
    {
        use shoop_dsp::backends::SynthBackend;
        for rate in [0., 7999., 96001., f32::NAN, f32::INFINITY] {
            assert!(SynthBackend::prepare(rate, 64).is_err());
        }
        assert!(SynthBackend::prepare(48000., 0).is_err());
        for rate in [8000., 44100., 96000.] {
            assert!(SynthBackend::prepare(rate, 64).is_ok());
        }
    }
}

#[cfg(feature = "builtin-fx")]
#[shoop_test]
fn incremental_retirement_restores_a_clean_rack_without_heap_operations() {
    use shoop_dsp::backends::{FxBackend, STAGE_BASE, WIDTH};
    for channels in [1, 2, 8] {
        let mut h = Harness::new(FxBackend::prepare(48000., 32, channels).unwrap(), 32);
        h.control(STAGE_BASE + 5, 1.);
        h.input.fill(1.);
        for _ in 0..20 {
            h.render();
        }
        h.node.abort(Frame(h.frame as u64)).unwrap();
        let mut steps = 0;
        assert_no_heap(|| loop {
            let done = h
                .node
                .inactive_backend_mut()
                .unwrap()
                .reset_step(steps)
                .unwrap();
            steps += 1;
            assert!(steps <= 84);
            if done {
                break;
            }
        });
        assert!(steps > channels * 6);
        for id in [0, 1, STAGE_BASE, STAGE_BASE + 5, WIDTH] {
            let fresh = FxBackend::prepare(48000., 32, channels).unwrap();
            assert_eq!(h.node.backend().parameter(id), fresh.parameter(id));
        }
        h.node.activate_prepared(Frame(h.frame as u64)).unwrap();
        h.input.fill(0.);
        assert_no_heap(|| {
            for _ in 0..20 {
                h.render();
                assert!(h.output.iter().all(|sample| *sample == 0.));
            }
        });
    }
}
