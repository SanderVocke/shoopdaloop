use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    Frame,
};
use shoop_engine_service::{dsp, prepare, Config, ExecutionStatus};
use shoop_wasm_test_support::{
    heap::{assert_no_heap, TrackingAllocator},
    shoop_test,
};
#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[shoop_test]
fn fixed_service_preparation_buffers_hosts_and_applies_rounded_dsp_controls() {
    for q in [16, 32, 64, 128] {
        let id = |slot| dsp::PresetId::from_parts(slot, 1).unwrap();
        let presets = [1., 2.]
            .into_iter()
            .enumerate()
            .map(|(slot, gain)| dsp::Preset {
                id: id(slot as u32),
                live_gain: [gain; 2],
                playback_gain: [0.; 2],
                swap_channels: false,
                delay_frames: 0,
            })
            .collect();
        let mut prepared = prepare(
            Config {
                quantum: q,
                max_frames: 64,
                ..Config::default()
            },
            presets,
            id(0),
        )
        .unwrap();
        assert_eq!(prepared.render.quantum().frames(), q);
        assert_eq!(prepared.render.buffering_latency(), q);
        let ticket = assert_no_heap(|| {
            prepared.client.dsp_at(
                Frame(1),
                dsp::Operation::SelectPreparedPreset {
                    preset: id(1),
                    expected_revision: 1,
                },
            )
        })
        .unwrap();
        let input = [1.; 64];
        let midi = [MidiBuffer::new(2048, 8192).unwrap()];
        let mut midi_out = [MidiBuffer::new(2048, 8192).unwrap()];
        let mut samples = Vec::new();
        while prepared.render.next_frame().0 < 4 * q as u64 {
            let mut left = [99.; 64];
            let mut right = [99.; 64];
            assert_no_heap(|| {
                prepared.render.process(
                    AudioBlock {
                        frame: prepared.render.next_frame(),
                        frames: 64,
                        inputs: &[&input, &input],
                        outputs: &mut [&mut left, &mut right],
                    },
                    MidiBlock {
                        inputs: &midi,
                        outputs: &mut midi_out,
                    },
                )
            })
            .unwrap();
            assert_eq!(left, right);
            samples.extend_from_slice(&left);
        }
        assert!(samples[..q].iter().all(|&v| v == 0.));
        assert!(samples[q..2 * q].iter().all(|&v| v == 1.));
        assert!(samples[2 * q..].iter().all(|&v| v == 2.));
        let mut found = false;
        while let Some(execution) = assert_no_heap(|| prepared.client.try_execution()) {
            if execution.id == Some(ticket.id()) {
                let ExecutionStatus::Executed(raw) = execution.status else {
                    panic!("execution")
                };
                let receipt = raw.processor_result.unwrap().unwrap();
                assert_eq!(receipt.requested_frame, Frame(1));
                assert_eq!(receipt.frame, Frame(q as u64));
                found = true;
            }
        }
        assert!(found);
        drop(prepared.client);
        let mut stopped = prepared.render.into_stopped();
        stopped.cleanup().unwrap();
        assert_eq!(
            stopped.resource_usage().free_pages,
            Config::default().resource_pages
        );
    }
}
