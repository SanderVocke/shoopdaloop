use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use rt_buffered::{
    AsyncBatch, AsyncCompletion, AsyncOutput, AsyncTransport, Backend, Batch, BufferLimits,
    BufferedProcessor, Lifecycle, MidiControls, MidiLimits, Timing,
};
use rt_runtime::{
    buffer::{AudioBlock, AudioPorts, MidiBlock, MidiBuffer},
    executor::{GraphRuntime, PreparedNode},
    graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType},
    planner::NoPlanner,
    resource::ResourceStore,
    Config, Error, Frame, NodeId, Quantum,
};
use shoop_wasm_test_support::{
    heap::{assert_no_heap, TrackingAllocator},
    shoop_test,
};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

const Q: usize = 16;
const PORTS: PortCounts = PortCounts {
    audio_inputs: 1,
    audio_outputs: 1,
    midi_inputs: 1,
    midi_outputs: 1,
};
const LIMITS: BufferLimits = BufferLimits {
    midi_input: MidiLimits {
        events_per_quantum: 4,
        bytes_per_quantum: 12,
    },
    midi_output: MidiLimits {
        events_per_quantum: 4,
        bytes_per_quantum: 12,
    },
    parameters_per_quantum: 1,
};

const COMPLETE: usize = 1;
const REPEAT: usize = 2;
const WRONG_SHAPE: usize = 3;
const NAN_AUDIO: usize = 4;
const MIDI_OVERFLOW: usize = 5;
const FAILED: usize = 6;
const COMPLETE_ALL: usize = usize::MAX;
const REPEAT_FOREVER: usize = usize::MAX - 1;

struct Job {
    tag: Option<rt_buffered::BatchTag>,
    audio: Vec<f32>,
    midi: [MidiBuffer; 1],
}

struct Worker {
    jobs: Vec<Job>,
    step: Arc<AtomicUsize>,
    dry_on_missing: bool,
}

impl AsyncTransport for Worker {
    fn try_submit(&mut self, batch: AsyncBatch<'_>) -> Result<(), Error> {
        let slot = self
            .jobs
            .iter_mut()
            .find(|job| job.tag.is_none())
            .ok_or(Error::Full)?;
        slot.audio.copy_from_slice(batch.audio);
        for event in batch.parameters {
            slot.audio[event.offset as usize] *= event.value;
        }
        slot.midi[0].clear();
        for index in 0..batch.midi[0].len() {
            let (offset, packet) = batch.midi[0].get(index).unwrap();
            slot.midi[0].push(offset, batch.frames, packet)?;
        }
        slot.tag = Some(batch.tag);
        Ok(())
    }

    fn try_completion(&mut self) -> Option<AsyncCompletion<'_>> {
        let step = match self.step.load(Ordering::Relaxed) {
            COMPLETE_ALL => self.jobs.iter().position(|job| job.tag.is_some())? * 8 + COMPLETE,
            REPEAT_FOREVER => REPEAT,
            _ => self.step.swap(0, Ordering::Relaxed),
        };
        if step == 0 {
            return None;
        }
        let job = self.jobs.get_mut(step / 8)?;
        let tag = job.tag?;
        let kind = step % 8;
        if kind != REPEAT {
            job.tag = None;
        }
        if kind == NAN_AUDIO {
            job.audio[0] = f32::NAN;
        }
        if kind == MIDI_OVERFLOW {
            job.midi[0].clear();
            for note in 0..5 {
                job.midi[0]
                    .push(0, job.audio.len(), &[0x90, note, 127])
                    .unwrap();
            }
        }
        Some(AsyncCompletion {
            tag,
            output: if kind == FAILED {
                Err(Error::Disconnected)
            } else {
                Ok(AsyncOutput {
                    frames: job.audio.len() + usize::from(kind == WRONG_SHAPE),
                    audio: &job.audio,
                    midi: &job.midi,
                })
            },
        })
    }
}

impl Backend for Worker {
    fn dry_on_missing(&self) -> bool {
        self.dry_on_missing
    }
    fn ports(&self) -> PortCounts {
        PORTS
    }

    fn async_transport(&mut self) -> Option<&mut dyn AsyncTransport> {
        Some(self)
    }

    fn validate_parameter(&self, id: u32, value: f32) -> Result<(), Error> {
        if id == 0 && value.is_finite() {
            Ok(())
        } else {
            Err(Error::InvalidParameter)
        }
    }

    fn process(&mut self, _: &mut Batch<'_, '_>) -> Result<(), Error> {
        panic!("worker-backed nodes execute through the transport")
    }

    fn reset(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

struct Harness {
    node: BufferedProcessor<Worker>,
    step: Arc<AtomicUsize>,
    input: MidiBuffer,
    output: MidiBuffer,
    deferred: MidiBuffer,
    current: MidiBuffer,
}

impl Harness {
    fn new(m: usize, s: usize, capacity: usize) -> Self {
        Self::with_fallback(m, s, capacity, false)
    }

    fn with_fallback(m: usize, s: usize, capacity: usize, dry_on_missing: bool) -> Self {
        let quantum = Quantum::new(Q).unwrap();
        let timing = Timing {
            batch_quanta: m,
            slack_quanta: s,
        };
        let step = Arc::new(AtomicUsize::new(0));
        let worker = Worker {
            jobs: (0..capacity)
                .map(|_| Job {
                    tag: None,
                    audio: vec![0.; Q * m],
                    midi: [MidiBuffer::new(20, 60).unwrap()],
                })
                .collect(),
            step: step.clone(),
            dry_on_missing,
        };
        let node = BufferedProcessor::with_backend(worker, quantum, timing, LIMITS).unwrap();
        assert_eq!(
            node.storage_bytes(),
            LIMITS
                .async_storage_bytes_for(PORTS, quantum, timing)
                .unwrap()
        );
        if m == 1 {
            assert!(
                node.storage_bytes() > LIMITS.storage_bytes_for(PORTS, quantum, timing).unwrap()
            );
        }
        Self {
            node,
            step,
            input: MidiBuffer::new(4, 12).unwrap(),
            output: MidiBuffer::new(4, 12).unwrap(),
            deferred: MidiBuffer::new(4, 12).unwrap(),
            current: MidiBuffer::new(4, 12).unwrap(),
        }
    }

    fn make_ready(&self, slot: usize, kind: usize) {
        self.step.store(slot * 8 + kind, Ordering::Relaxed);
    }

    fn tick(&mut self, samples: [f32; Q]) -> [f32; Q] {
        let mut output = [f32::NAN; Q];
        let mut audio = AudioPorts::new(&samples, &mut output, Q, 0, Q).unwrap();
        self.output.clear();
        let frame = self.node.next_frame();
        assert_no_heap(|| {
            self.node
                .process_ports_tick_controls(
                    frame,
                    Quantum::new(Q).unwrap(),
                    &mut audio,
                    core::slice::from_ref(&self.input),
                    core::slice::from_mut(&mut self.output),
                    Some(MidiControls {
                        deferred: &self.deferred,
                        current: &self.current,
                    }),
                    None,
                )
                .unwrap();
        });
        output
    }
}

#[shoop_test]
fn async_buffering_owns_midi_and_releases_at_the_original_source_time() {
    let mut host = Harness::new(2, 1, 3);
    host.input.push(3, Q, &[0x90, 60, 100]).unwrap();
    host.deferred.push(3, Q, &[0xb0, 1, 16]).unwrap();
    host.current.push(3, Q, &[0xb0, 1, 32]).unwrap();
    assert_eq!(host.tick([1.; Q]), [0.; Q]);
    host.deferred.clear();
    host.current.clear();
    host.input.clear();
    host.input.push(1, Q, &[0x80, 60, 0]).unwrap();
    host.node
        .schedule_parameter(Frame(Q as u64 + 1), 0, 3.)
        .unwrap();
    assert_eq!(host.tick([2.; Q]), [0.; Q]);
    host.input.clear();
    assert!(host.node.completed_audio(Frame(0), 0).is_none());
    host.make_ready(0, COMPLETE);
    assert_eq!(host.tick([9.; Q]), [0.; Q]);
    assert!(host.node.last_release().is_none());
    let mut expected = [2.; Q];
    expected[1] = 6.;
    let full_batch = [[1.; Q], expected].concat();
    assert_eq!(
        host.node.completed_audio(Frame(0), 0).unwrap().1,
        &full_batch
    );
    assert_eq!(host.tick([9.; Q]), [1.; Q]);
    assert_eq!(host.output.len(), 3);
    assert_eq!(host.output.get(0), Some((3, &[0xb0, 1, 16][..])));
    assert_eq!(host.output.get(1), Some((3, &[0x90, 60, 100][..])));
    assert_eq!(host.output.get(2), Some((3, &[0xb0, 1, 32][..])));
    assert_eq!(host.tick([9.; Q]), expected);
    assert_eq!(host.output.get(0), Some((1, &[0x80, 60, 0][..])));
    let release = host.node.last_release().unwrap();
    assert!(release.available);
    assert_eq!((release.tag.source, release.offset), (Frame(0), Q));
}

#[shoop_test]
fn async_buffering_expires_whole_batches_and_resumes_after_recovering_notes() {
    let mut host = Harness::new(2, 1, 3);
    assert_eq!(host.tick([1.; Q]), [0.; Q]);
    assert_eq!(host.tick([2.; Q]), [0.; Q]);
    assert_eq!(host.tick([3.; Q]), [0.; Q]);
    assert_eq!(host.tick([4.; Q]), [0.; Q]);
    assert!(host.output.is_recovery());
    assert_eq!(host.output.get(0), Some((0, &[0xb0, 120, 0][..])));
    assert!(!host.node.last_release().unwrap().available);
    host.make_ready(0, COMPLETE);
    assert_eq!(host.tick([5.; Q]), [0.; Q]);
    assert!(!host.output.is_recovery());
    assert!(host.node.completed_audio(Frame(0), 0).is_none());
    host.make_ready(1, COMPLETE);
    assert_eq!(host.tick([6.; Q]), [3.; Q]);
    assert_eq!(host.tick([7.; Q]), [4.; Q]);
    let stats = host.node.async_stats().unwrap();
    assert_eq!((stats.missed, stats.stale, stats.completed), (1, 1, 1));
    assert_eq!(stats.last_completion_frames, Q as u64);
    assert_eq!(stats.peak_completion_frames, Q as u64);
    assert_eq!(host.node.lifecycle(), Lifecycle::Running);
}

#[shoop_test]
fn delayed_dry_fallback_keeps_missing_wet_release_unavailable() {
    let mut host = Harness::with_fallback(2, 1, 3, true);
    assert_eq!(host.tick([1.; Q]), [0.; Q]);
    assert_eq!(host.tick([2.; Q]), [0.; Q]);
    assert_eq!(host.tick([3.; Q]), [0.; Q]);
    assert_eq!(host.tick([4.; Q]), [1.; Q]);
    assert!(host.output.is_recovery());
    assert!(!host.node.last_release().unwrap().available);
    assert!(host.node.completed_audio(Frame(0), 0).is_none());
    assert_eq!(host.tick([5.; Q]), [2.; Q]);
    assert!(!host.node.last_release().unwrap().available);
}

#[shoop_test]
fn async_buffering_releases_out_of_order_results_in_order_and_discards_duplicates() {
    let mut host = Harness::new(1, 2, 5);
    host.tick([1.; Q]);
    host.input.push(3, Q, &[0x90, 61, 90]).unwrap();
    host.tick([2.; Q]);
    host.input.clear();
    host.make_ready(1, REPEAT);
    assert_eq!(host.tick([3.; Q]), [0.; Q]);
    host.make_ready(0, COMPLETE);
    assert_eq!(host.tick([4.; Q]), [1.; Q]);
    assert!(host.output.is_empty());
    host.make_ready(1, COMPLETE);
    assert_eq!(host.tick([5.; Q]), [2.; Q]);
    assert_eq!(host.output.get(0), Some((3, &[0x90, 61, 90][..])));
    assert_eq!(host.node.async_stats().unwrap().stale, 1);
}

#[shoop_test]
fn async_buffering_bounds_completion_polling_even_if_a_transport_repeats_one_forever() {
    let mut host = Harness::new(1, 0, 2);
    host.tick([1.; Q]);
    host.step.store(REPEAT_FOREVER, Ordering::Relaxed);
    assert_eq!(host.tick([2.; Q]), [1.; Q]);
    assert_eq!(host.node.async_stats().unwrap().stale, 7);
    assert_eq!(host.tick([3.; Q]), [0.; Q]);
    assert!(host.output.is_recovery());
    assert_eq!(host.node.async_stats().unwrap().stale, 15);
}

#[shoop_test]
fn async_buffering_bounds_pressure_and_isolates_malformed_or_failed_results() {
    for failure in [WRONG_SHAPE, NAN_AUDIO, MIDI_OVERFLOW, FAILED] {
        let mut host = Harness::new(1, 1, 1);
        host.tick([1.; Q]);
        host.tick([2.; Q]);
        host.make_ready(0, failure);
        assert_eq!(host.tick([3.; Q]), [0.; Q]);
        assert!(host.output.is_recovery());
        host.make_ready(0, COMPLETE);
        assert_eq!(host.tick([4.; Q]), [0.; Q]);
        assert!(host.output.is_recovery());
        assert_eq!(host.tick([5.; Q]), [3.; Q]);
        let stats = host.node.async_stats().unwrap();
        assert_eq!(stats.invalid, u64::from(failure != FAILED));
        assert_eq!(stats.failed, u64::from(failure == FAILED));
        assert_eq!(stats.rejected, 2);
        assert_eq!((stats.missed, stats.peak_pending), (2, 1));
    }
}

#[shoop_test]
fn async_buffering_aborts_old_generations_and_drains_padded_tail() {
    let mut host = Harness::new(2, 1, 3);
    host.tick([1.; Q]);
    host.tick([1.; Q]);
    let old = host.node.generation();
    let restart = Frame((2 * Q) as u64);
    host.node.abort(restart).unwrap();
    host.node.activate_prepared(restart).unwrap();
    host.make_ready(0, COMPLETE);
    assert_eq!(host.tick([2.; Q]), [0.; Q]);
    assert_eq!(host.node.async_stats().unwrap().stale, 1);
    host.node.begin_drain().unwrap();
    assert_eq!(host.tick([9.; Q]), [0.; Q]);
    host.make_ready(0, COMPLETE);
    assert_eq!(host.tick([9.; Q]), [0.; Q]);
    assert_eq!(host.tick([9.; Q]), [2.; Q]);
    let release = host.node.last_release().unwrap();
    assert!(release.available);
    assert!(release.tag.generation > old);
    assert_eq!(release.tag.source, restart);
    assert_eq!(host.tick([9.; Q]), [0.; Q]);
    assert_eq!(host.node.lifecycle(), Lifecycle::Suspended);
    assert_eq!(host.node.async_stats().unwrap().pending, 0);
}

#[shoop_test]
fn async_buffering_matches_source_audio_and_midi_across_host_partitions() {
    let quantum = Quantum::new(Q).unwrap();
    for partition in [1, 11, 29, 64] {
        let Harness {
            node: processor,
            step,
            mut input,
            mut output,
            ..
        } = Harness::new(2, 1, 2);
        let delay = processor.latency_frames() + Q;
        step.store(COMPLETE_ALL, Ordering::Relaxed);
        let node = NodeId::from_parts(0, 1).unwrap();
        let config = Config {
            max_frames: 64,
            audio_inputs: 1,
            audio_outputs: 1,
            midi_inputs: 1,
            midi_outputs: 1,
            max_nodes: 1,
            max_ports: 8,
            max_edges: 4,
            midi_events_per_port: 4,
            midi_bytes_per_port: 12,
            ..Config::default()
        }
        .prepare()
        .unwrap();
        let (_, mut runtime) = GraphRuntime::<_, _, (), _>::prepare_with_planner_and_resources(
            config,
            quantum,
            vec![PreparedNode::new(node, processor)],
            NoPlanner,
            ResourceStore::new(config, quantum).unwrap(),
        )
        .unwrap();
        let mut connections = Vec::new();
        for kind in [PortType::Audio, PortType::Midi] {
            let host = Endpoint::host(kind, 0);
            let processor = Endpoint::node(node, kind, 0);
            connections.push(Connection {
                source: host,
                destination: processor,
            });
            connections.push(Connection {
                source: processor,
                destination: host,
            });
        }
        runtime
            .prepare_initial_graph(&GraphDescription {
                nodes: vec![node],
                connections,
            })
            .unwrap();
        runtime.prepare_host_buffering().unwrap();

        let note_on = [0x90, 60, 100];
        let note_off = [0x80, 60, 0];
        let notes = [(Q - 1, note_on), (Q, note_off)];
        let total = 7 * Q;
        let mut midi = Vec::new();
        for frame in (0..total).step_by(partition) {
            let frames = partition.min(total - frame);
            let source: Vec<_> = (frame..frame + frames)
                .map(|index| index as f32 + 1.)
                .collect();
            let mut sink = vec![f32::NAN; frames];
            input.clear();
            output.clear();
            for (at, packet) in notes {
                if (frame..frame + frames).contains(&at) {
                    input.push((at - frame) as u32, frames, &packet).unwrap();
                }
            }
            assert_no_heap(|| {
                runtime
                    .process(
                        AudioBlock {
                            frame: Frame(frame as u64),
                            frames,
                            inputs: &[&source],
                            outputs: &mut [&mut sink],
                        },
                        MidiBlock {
                            inputs: core::slice::from_ref(&input),
                            outputs: core::slice::from_mut(&mut output),
                        },
                    )
                    .unwrap()
            });
            for (index, sample) in sink.into_iter().enumerate() {
                let index = index + frame;
                assert_eq!(
                    sample,
                    index
                        .checked_sub(delay)
                        .map_or(0., |source| source as f32 + 1.)
                );
            }
            for index in 0..output.len() {
                let (at, packet) = output.get(index).unwrap();
                midi.push((frame + at as usize, packet.to_vec()));
            }
        }
        assert_eq!(
            midi,
            vec![
                (delay + Q - 1, note_on.to_vec()),
                (delay + Q, note_off.to_vec())
            ]
        );
    }
}
