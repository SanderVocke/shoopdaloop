use shoop_dsp::{DelayCopy, Processor};
use shoop_dsp_link::{
    memory, returns::Returns, service::Service, Block, Buffer, Descriptor, Endpoint, Error, Header,
    MidiEvent,
};
use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn descriptor() -> Descriptor {
    Descriptor {
        quantum: 3,
        dsp_frames: 5,
        max_callback: 7,
        service_allowance: 11,
        latency: 36,
        slots: 16,
        result_budget: 16,
        midi_capacity: 8,
        ..Descriptor::default()
    }
}
fn value(frame: u64, c: usize) -> f32 {
    (frame + c as u64 * 1000) as f32 / 8.
}

#[shoop_wasm_test_support::shoop_test]
fn non_network_q3_b5_service_preserves_exact_delay_and_midi() {
    let d = descriptor();
    let mut dsp = DelayCopy::prepare(d.dsp_config(), 7).unwrap();
    let mut service = Service::prepare(d).unwrap();
    let mut input = Block::prepare(d).unwrap();
    let mut received = 0;
    for step in 0..100 {
        input.set_header(Header {
            generation: d.generation,
            frame: step * 3,
            ..Header::default()
        });
        input.clear_midi();
        for c in 0..2 {
            for i in 0..3 {
                input.audio_mut()[c * 3 + i] = value(step * 3 + i as u64, c);
            }
        }
        for i in 0..3 {
            if (step * 3 + i) % 7 == 0 {
                input
                    .push_midi(MidiEvent::new(i as u32, 0, &[0x90, 60, 100]).unwrap())
                    .unwrap();
            }
        }
        assert_no_heap(|| {
            service.push(&input, &mut dsp, |out| {
                assert_eq!(out.header().frame, received);
                for c in 0..2 {
                    for i in 0..3 {
                        let frame = received + i as u64;
                        assert_eq!(
                            out.audio()[c * 3 + i],
                            if frame < 7 { 0. } else { value(frame - 7, c) }
                        );
                    }
                }
                for event in out.midi() {
                    assert_eq!((received + u64::from(event.frame)) % 7, 0);
                }
                received += 3;
            })
        })
        .unwrap();
    }
    assert_eq!(received, 300);
    assert_eq!(service.stats().dsp_blocks, 60);
    assert_eq!(dsp.info().latency, shoop_dsp::Latency::Known(7));
}

#[shoop_wasm_test_support::shoop_test]
fn credit_exhaustion_releases_every_lease_without_render_heap_work() {
    let d = Descriptor {
        slots: 2,
        result_budget: 2,
        ..descriptor()
    };
    let (mut render, mut service) = memory::pair(d).unwrap();
    let mut leases = Vec::with_capacity(2);
    assert_no_heap(|| {
        leases.push(render.acquire_input().unwrap());
        leases.push(render.acquire_input().unwrap());
        assert!(render.acquire_input().is_none());
        while let Some(lease) = leases.pop() {
            render.release_input(lease);
        }
        for _ in 0..2 {
            let lease = render.acquire_input().unwrap();
            assert!(render.submit(lease).is_ok());
        }
        assert!(render.acquire_input().is_none());
        for _ in 0..2 {
            let lease = service.receive().unwrap();
            service.release_input(lease);
        }
        for _ in 0..2 {
            let lease = service.acquire_output().unwrap();
            assert!(service.publish(lease).is_ok());
        }
        assert!(service.acquire_output().is_none());
        for _ in 0..2 {
            let lease = render.receive().unwrap();
            render.release_output(lease);
        }
    });
    drop(service);
    assert!(!render.available());
}

#[shoop_wasm_test_support::shoop_test]
fn frame_returns_commit_once_and_refuse_bad_identity_without_mutation() {
    let d = descriptor();
    let mut returns = Returns::prepare(d).unwrap();
    let mut input = Block::prepare(d).unwrap();
    let mut output = Block::prepare(d).unwrap();
    input.audio_mut().fill(17.);
    assert_no_heap(|| {
        returns.accept(&input).unwrap();
        assert_eq!(returns.accept(&input), Err(Error::Duplicate));
        assert_eq!(
            returns.consume_at(35, true, &mut output),
            Err(Error::Deadline)
        );
        assert!(returns.consume_at(36, true, &mut output).unwrap());
        assert!(output.audio().iter().all(|v| *v == 17.));
        assert_eq!(returns.accept(&input), Err(Error::Stale));
        assert!(!returns.consume_at(39, true, &mut output).unwrap());
        assert!(output.header().midi_reset);
        assert!(output.audio().iter().all(|v| *v == 0.));
        let mut h = input.header();
        h.generation += 1;
        input.set_header(h);
        assert_eq!(returns.accept(&input), Err(Error::Stale));
        h.generation = d.generation;
        h.frame = 1;
        input.set_header(h);
        assert_eq!(returns.accept(&input), Err(Error::Shape));
        h.frame = u64::from(d.quantum) * 1000;
        input.set_header(h);
        assert_eq!(returns.accept(&input), Err(Error::Future));
    });
    assert!(Descriptor { latency: 35, ..d }.validate().is_err());
    assert!(Descriptor { quantum: 0, ..d }.validate().is_err());
    assert!(Descriptor {
        start_frame: u64::MAX - 3,
        ..d
    }
    .validate()
    .is_err());
}

#[shoop_wasm_test_support::shoop_test]
fn bounded_gap_fills_source_samples_without_inventing_unbounded_progress() {
    let d = descriptor();
    let mut dsp = DelayCopy::prepare(d.dsp_config(), 0).unwrap();
    let mut service = Service::prepare(d).unwrap();
    let mut input = Block::prepare(d).unwrap();
    input.audio_mut().fill(1.);
    let mut outputs = 0;
    for frame in [0, 6, 9, 12] {
        input.set_header(Header {
            generation: d.generation,
            frame,
            ..Header::default()
        });
        service
            .push(&input, &mut dsp, |out| {
                for i in 0..3 {
                    let frame = out.header().frame + i as u64;
                    assert_eq!(
                        out.audio()[i],
                        if (3..6).contains(&frame) { 0. } else { 1. }
                    );
                }
                outputs += 1;
            })
            .unwrap();
    }
    assert_eq!(outputs, 5);
    assert_eq!(service.stats().concealed_frames, 3);
    input.set_header(Header {
        generation: d.generation,
        frame: 1000,
        ..Header::default()
    });
    assert_eq!(
        service.push(&input, &mut dsp, |_| panic!("no output after refusal")),
        Err(Error::Shape)
    );
    input.set_header(Header {
        generation: d.generation,
        frame: 999,
        ..Header::default()
    });
    assert_eq!(
        service.push(&input, &mut dsp, |_| panic!("no output after refusal")),
        Err(Error::Future)
    );
}

struct AudioOnly {
    config: shoop_dsp::Config,
    playing: bool,
}
impl Processor for AudioOnly {
    fn info(&self) -> shoop_dsp::Info {
        shoop_dsp::Info {
            config: self.config,
            latency: shoop_dsp::Latency::Known(0),
            midi: true,
            state: false,
            ui: false,
        }
    }
    fn process(&mut self, block: shoop_dsp::Block<'_>) -> Result<(), shoop_dsp::Error> {
        block.validate(self.config)?;
        self.playing &= block.transport.playing;
        block.output.copy_from_slice(block.input);
        block.midi_output.clear();
        Ok(())
    }
    fn reset(&mut self) -> Result<(), shoop_dsp::Error> {
        Ok(())
    }
}

#[shoop_wasm_test_support::shoop_test]
fn midi_recovery_survives_a_processor_that_filters_all_midi_output() {
    let d = descriptor();
    let mut dsp = AudioOnly {
        config: d.dsp_config(),
        playing: true,
    };
    let mut service = Service::prepare(d).unwrap();
    let mut input = Block::prepare(d).unwrap();
    let mut resets = 0;
    for frame in [0, 3] {
        input.set_header(Header {
            generation: d.generation,
            frame,
            midi_reset: frame == 0,
            transport: shoop_dsp_link::Transport {
                playing: true,
                tempo: None,
            },
            ..Header::default()
        });
        service
            .push(&input, &mut dsp, |out| {
                assert!(out.header().midi_reset);
                assert!(out.midi().is_empty());
                resets += 1;
            })
            .unwrap();
    }
    assert_eq!(resets, 1);
}

#[shoop_wasm_test_support::shoop_test]
fn an_initial_input_gap_uses_the_known_transport_context() {
    let d = descriptor();
    let mut dsp = AudioOnly {
        config: d.dsp_config(),
        playing: true,
    };
    let mut service = Service::prepare(d).unwrap();
    let mut input = Block::prepare(d).unwrap();
    input.set_header(Header {
        generation: d.generation,
        frame: 3,
        transport: shoop_dsp_link::Transport {
            playing: true,
            tempo: Some(120.),
        },
        ..Header::default()
    });
    service.push(&input, &mut dsp, |_| {}).unwrap();
    assert!(dsp.playing);
}
