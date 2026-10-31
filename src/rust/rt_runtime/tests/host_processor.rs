#[path = "../../../../tests/rt_runtime_host/src/node.rs"]
mod node;
mod support;
use rt_runtime::buffer::{AudioPorts, MidiBuffer};
use rt_runtime::graph::PortCounts;
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::resource::{Description, Format, ResourceStore};
use rt_runtime::{Budgets, Config, Error, Frame, Revision};
use shoop_wasm_test_support::shoop_test;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
const QUANTUM: usize = 128;
fn ports() -> PortCounts {
    PortCounts {
        audio_inputs: 0,
        audio_outputs: 2,
        midi_inputs: 1,
        midi_outputs: 1,
    }
}
fn exercise(pages: usize, midi_reset: bool) {
    let config = Config {
        max_frames: QUANTUM,
        resource_slots: 8,
        resource_pages: pages,
        page_bytes: 64,
        budgets: Budgets {
            transfer_bytes: 2048,
            validation_steps: 16,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let mut store = ResourceStore::new(config).unwrap();
    let playback = store.create().unwrap();
    let audio_capture = store.create().unwrap();
    let midi_capture = store.create().unwrap();
    let upload = store
        .begin_upload(
            playback,
            Revision(0),
            Description {
                format: Format::Audio { channels: 1 },
                frames: 16,
                bytes: 64,
            },
        )
        .unwrap();
    let data: Vec<_> = [1f32; 16].into_iter().flat_map(f32::to_le_bytes).collect();
    store.begin_callback();
    store.write_upload(upload, 0, &data).unwrap();
    store.finish_upload(upload).unwrap();
    store.commit(upload).unwrap();
    let snapshot = store.snapshot(playback, Frame(0)).unwrap();
    let cursor = store.cursor(snapshot.id).unwrap();
    let mut processor = node::Node {
        playback,
        audio_capture,
        midi_capture,
        snapshot,
        cursor,
        frames: 0,
        gain: 1.,
    };
    assert_eq!(processor.ports(), ports());
    let audio = store
        .begin_capture(
            audio_capture,
            Revision(0),
            Format::Audio { channels: 1 },
            Frame(0),
        )
        .unwrap();
    let midi = if midi_reset {
        Some(
            store
                .begin_capture(midi_capture, Revision(0), Format::Midi, Frame(0))
                .unwrap(),
        )
    } else {
        None
    };
    let mut input = [MidiBuffer::new(32, 96).unwrap()];
    let mut output = [MidiBuffer::new(8, 64).unwrap()];
    if midi_reset {
        for channel in 0..16 {
            for controller in [120, 123] {
                input[0]
                    .push(0, QUANTUM, &[0xb0 | channel, controller, 0])
                    .unwrap();
            }
        }
    }
    let mut samples = [0.; QUANTUM * 2];
    for callback in 0..2 {
        store.begin_callback();
        output[0].clear();
        let result = support::assert_no_heap(|| {
            processor.process(&mut ProcessBlock {
                frame: Frame(callback * QUANTUM as u64),
                frames: QUANTUM,
                audio: AudioPorts::new(&[], &mut samples, QUANTUM, 0, QUANTUM).unwrap(),
                midi_inputs: &input,
                midi_outputs: &mut output,
                resources: store.io(),
                decision: None,
                observations: rt_runtime::planner::ObservationWriter::disabled(&mut 0),
            })
        });
        assert_eq!(
            result,
            if midi_reset && callback == 0 {
                Err(Error::Full)
            } else {
                Ok(())
            }
        );
        assert_eq!(processor.frames, (callback + 1) * QUANTUM as u64);
        assert_eq!(samples[..QUANTUM], [1.; QUANTUM]);
        assert_eq!(samples[QUANTUM..], [-1.; QUANTUM]);
        let info = store
            .info(if midi_reset {
                midi_capture
            } else {
                audio_capture
            })
            .unwrap();
        assert_eq!(info.description.bytes, 0);
        assert_eq!(
            info.capture.unwrap().incomplete,
            Some(if midi_reset {
                Error::Budget
            } else {
                Error::Capacity
            })
        );
        assert!(!info.capture.unwrap().active);
        assert!(store.work().copied_bytes <= 2048 && store.work().validation_steps <= 16);
        input[0].clear();
    }
    store.stop_capture(audio).unwrap();
    if let Some(midi) = midi {
        store.stop_capture(midi).unwrap();
    }
    store.release_snapshot(snapshot.id).unwrap();
    for resource in [playback, audio_capture, midi_capture] {
        store.remove(resource).unwrap();
    }
    for _ in 0..64 {
        store.begin_callback();
        support::assert_no_heap(|| store.reclaim());
    }
    assert_eq!(store.usage().free_pages, pages);
    assert_eq!(store.usage().free_versions, 8);
    assert_eq!(store.usage().free_snapshots, 8);
}
#[shoop_test]
fn worklet_processor_keeps_audio_running_after_capture_pool_exhaustion() {
    exercise(2, false);
}
#[shoop_test]
fn worklet_processor_advances_state_through_midi_reset_and_capture_budget_exhaustion() {
    exercise(64, true);
}
