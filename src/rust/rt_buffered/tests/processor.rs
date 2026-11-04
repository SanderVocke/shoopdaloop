use rt_buffered::{BufferedProcessor, MidiLimits, Timing};
use rt_runtime::{
    buffer::{AudioPorts, MidiBuffer},
    fixed::{Quantum, Tick},
    Error, Frame,
};
use shoop_wasm_test_support::{
    heap::{assert_no_heap, TrackingAllocator},
    shoop_test,
};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

struct Harness {
    node: BufferedProcessor,
    input: Vec<f32>,
    output: Vec<f32>,
    midi_input: MidiBuffer,
    midi_output: MidiBuffer,
}

impl Harness {
    fn new(q: usize, m: usize, s: usize, limits: MidiLimits) -> Self {
        Self {
            node: BufferedProcessor::prepare(
                Quantum::new(q).unwrap(),
                Timing {
                    batch_quanta: m,
                    slack_quanta: s,
                },
                limits,
            )
            .unwrap(),
            input: vec![0.; 2 * q],
            output: vec![99.; 2 * q],
            midi_input: MidiBuffer::new(16, 256).unwrap(),
            midi_output: MidiBuffer::new(16, 256).unwrap(),
        }
    }
    fn render(&mut self, frame: u64) -> Result<(), Error> {
        let quantum = self.node.timing().quantum();
        let mut audio = AudioPorts::new(
            &self.input,
            &mut self.output,
            quantum.frames(),
            0,
            quantum.frames(),
        )
        .unwrap();
        self.node.process_tick(
            Tick {
                frame: Frame(frame),
                quantum,
            },
            &mut audio,
            &self.midi_input,
            &mut self.midi_output,
        )
    }
}

fn sample(channel: usize, frame: usize) -> f32 {
    if channel == 0 {
        (frame as i32 % 23 - 11) as f32 * 0.25
    } else {
        (frame as i32 % 13 - 3) as f32 * -0.5
    }
}

#[shoop_test]
fn buffered_stream_mapping_startup_batch_counts_and_slot_reuse() {
    for (q, m, s) in [
        (1, 1, 0),
        (16, 1, 0),
        (16, 2, 1),
        (16, 3, 7),
        (32, 3, 0),
        (64, 2, 1),
        (128, 3, 7),
    ] {
        let mut h = Harness::new(q, m, s, MidiLimits::default());
        let latency = (m + s) * q;
        for tick in 0..m + s + 12 * m {
            let frame = tick * q;
            for c in 0..2 {
                for n in 0..q {
                    h.input[c * q + n] = sample(c, frame + n);
                }
            }
            h.render(frame as u64).unwrap();
            for c in 0..2 {
                for n in 0..q {
                    let at = frame + n;
                    let expected = if at < latency {
                        0.
                    } else {
                        2. * sample(c, at - latency)
                    };
                    assert_eq!(
                        h.output[c * q + n],
                        expected,
                        "q={q},m={m},s={s},frame={at},channel={c}"
                    );
                }
            }
            assert_eq!(h.node.completed_batches(), ((tick + 1) / m) as u64);
        }
    }
}

#[shoop_test]
fn buffered_midi_offsets_boundaries_equal_time_order_and_slack() {
    let mut h = Harness::new(16, 3, 7, MidiLimits::default());
    let source: &[(u64, &[u8], &[u8])] = &[
        (0, &[0x9f, 60, 100], &[0x90, 60, 100]),
        (15, &[0x90, 61, 100], &[0x91, 61, 100]),
        (15, &[0x90, 62, 0], &[0x91, 62, 0]),
        (16, &[0x80, 61, 0], &[0x81, 61, 0]),
        (47, &[0xf8], &[0xf8]),
        (48, &[0xf0, 1, 2, 3, 0xf7], &[0xf0, 1, 2, 3, 0xf7]),
    ];
    let mut received = Vec::new();
    for frame in (0..256).step_by(16) {
        h.midi_input.clear();
        h.midi_output.clear();
        for &(at, packet, _) in source {
            if at >= frame && at < frame + 16 {
                h.midi_input.push((at - frame) as u32, 16, packet).unwrap();
            }
        }
        h.render(frame).unwrap();
        for i in 0..h.midi_output.len() {
            let (at, bytes) = h.midi_output.get(i).unwrap();
            received.push((frame + at as u64, bytes.to_vec()));
        }
    }
    assert_eq!(
        received,
        source
            .iter()
            .map(|&(at, _, bytes)| (at + 160, bytes.to_vec()))
            .collect::<Vec<_>>()
    );
}

#[shoop_test]
fn buffered_capacity_preflight_is_atomic_at_exact_event_and_byte_limits() {
    let limits = MidiLimits {
        events_per_quantum: 2,
        bytes_per_quantum: 6,
    };
    let mut h = Harness::new(16, 1, 0, limits);
    for key in [60, 61, 62] {
        h.midi_input.push(0, 16, &[0x90, key, 100]).unwrap();
    }
    h.input.fill(1.);
    assert_eq!(h.render(0), Err(Error::Capacity));
    assert_eq!(h.output, vec![99.; 32]);
    assert_eq!(h.node.next_frame(), Frame(0));
    h.midi_input.clear();
    h.midi_input
        .push(0, 16, &[0xf0, 1, 2, 3, 4, 5, 0xf7])
        .unwrap();
    assert_eq!(h.render(0), Err(Error::Capacity));
    h.midi_input.clear();
    for key in [60, 61] {
        h.midi_input.push(15, 16, &[0x90, key, 100]).unwrap();
    }
    h.render(0).unwrap();
    h.midi_input.clear();
    h.midi_output = MidiBuffer::new(1, 6).unwrap();
    assert_eq!(h.render(16), Err(Error::Capacity));
    assert!(h.midi_output.is_empty());
    assert_eq!(h.node.next_frame(), Frame(16));
    h.midi_output = MidiBuffer::new(2, 5).unwrap();
    assert_eq!(h.render(16), Err(Error::Capacity));
    assert!(h.midi_output.is_empty());
    assert_eq!(h.node.next_frame(), Frame(16));
    h.midi_output = MidiBuffer::new(2, 6).unwrap();
    h.render(16).unwrap();
    assert_eq!(h.output, vec![2.; 32]);
    assert_eq!(h.midi_output.len(), 2);
    assert_eq!(h.midi_output.byte_len(), 6);
    assert_eq!(h.midi_output.get(1), Some((15, &[0x91, 61, 100][..])));
    assert!(matches!(
        BufferedProcessor::prepare(
            Quantum::new(16).unwrap(),
            Timing::default(),
            MidiLimits {
                events_per_quantum: usize::MAX,
                bytes_per_quantum: 0
            }
        ),
        Err(Error::Capacity)
    ));
    assert!(matches!(
        BufferedProcessor::prepare(
            Quantum::new(1).unwrap(),
            Timing {
                batch_quanta: 1,
                slack_quanta: 262_143
            },
            MidiLimits::default()
        ),
        Err(Error::Capacity)
    ));
}

#[shoop_test]
fn buffered_reset_and_discontinuity_refusal_preserve_stream_identity() {
    let mut h = Harness::new(16, 2, 1, MidiLimits::default());
    h.input.fill(3.);
    h.midi_input.push(0, 16, &[0x90, 60, 100]).unwrap();
    h.render(0).unwrap();
    assert_eq!(h.render(32), Err(Error::InvalidFrame));
    assert_eq!(h.render(0), Err(Error::InvalidFrame));
    assert_eq!(h.node.next_frame(), Frame(16));
    h.render(16).unwrap();
    h.render(32).unwrap();
    assert_eq!(h.node.completed_batches(), 1);
    let generation = h.node.generation();
    assert_eq!(h.node.reset(Frame(1)), Err(Error::InvalidFrame));
    assert_eq!(h.node.reset(Frame(u64::MAX - 15)), Err(Error::Exhausted));
    assert_eq!(h.node.generation(), generation);
    h.node.reset(Frame(160)).unwrap();
    assert_eq!(h.node.generation(), generation + 1);
    assert_eq!(h.node.completed_batches(), 0);
    h.input.fill(0.);
    h.midi_input.clear();
    for frame in (160..320).step_by(16) {
        h.midi_output.clear();
        h.render(frame).unwrap();
        assert_eq!(h.output, vec![0.; 32]);
        assert!(h.midi_output.is_empty());
    }
    let mut audio = AudioPorts::new(&h.input, &mut h.output, 16, 0, 16).unwrap();
    assert_eq!(
        h.node.process_tick(
            Tick {
                frame: Frame(320),
                quantum: Quantum::new(32).unwrap()
            },
            &mut audio,
            &h.midi_input,
            &mut h.midi_output
        ),
        Err(Error::InvalidFrame)
    );
    let mut malformed = AudioPorts::new(&h.input, &mut h.output, 16, 0, 8).unwrap();
    assert_eq!(
        h.node.process_tick(
            Tick {
                frame: Frame(320),
                quantum: Quantum::new(16).unwrap()
            },
            &mut malformed,
            &h.midi_input,
            &mut h.midi_output
        ),
        Err(Error::InvalidShape)
    );
    assert_eq!(h.node.next_frame(), Frame(320));
}

#[shoop_test]
fn buffered_rendering_has_no_heap_work_with_maximum_admitted_midi() {
    let limits = MidiLimits {
        events_per_quantum: 4,
        bytes_per_quantum: 12,
    };
    let mut h = Harness::new(16, 3, 7, limits);
    for n in 0..4 {
        h.midi_input
            .push(n, 16, &[0x90, 60 + n as u8, 100])
            .unwrap();
    }
    for tick in 0..80 {
        h.midi_output.clear();
        assert_no_heap(|| h.render(tick * 16)).unwrap();
        assert_eq!(h.midi_output.len(), if tick < 10 { 0 } else { 4 });
        if tick >= 10 {
            assert_eq!(h.midi_output.get(3), Some((3, &[0x91, 63, 100][..])));
        }
    }
}
