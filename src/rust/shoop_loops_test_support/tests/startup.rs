#[path = "support/mixed.rs"]
pub mod mixed;
use mixed::{assemble, Content as InitialContent, MidiPacket as InitialMidi, Tap};
use rt_runtime::transport::Lane;
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    midi_state::MidiState,
    Error, Frame,
};
use shoop_example_dsp as dsp;
use shoop_loops::runtime::{Operation as Op, Reply};

use shoop_loops::{runtime::Failure, Mode, Timing, Transition};
use shoop_wasm_test_support::{heap, shoop_test};

#[global_allocator]
static ALLOCATOR: heap::TrackingAllocator = heap::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
fn preset() -> dsp::Preset {
    dsp::Preset {
        id: dsp::PresetId::from_parts(0, 1).unwrap(),
        live_gain: [1.; 2],
        playback_gain: [1.; 2],
        swap_channels: false,
        delay_frames: 0,
    }
}
fn content() -> InitialContent {
    let mut initial = MidiState::default();
    initial.apply_packet(&[0x90, 62, 40], 3).unwrap();
    InitialContent {
        audio: [vec![-10., -20.], vec![100., 200.]],
        midi_initial: initial,
        midi: vec![
            InitialMidi {
                frame: 0,
                bytes: vec![0x90, 64, 70],
            },
            InitialMidi {
                frame: 1,
                bytes: vec![0x80, 64, 30],
            },
        ],
    }
}
#[shoop_test]
fn startup_import_is_real_stereo_and_indexed_midi_before_frame_zero_without_resource_handle_leaks()
{
    let p = preset();
    let prepared = assemble(128, Tap::PostDsp, vec![p], p.id, Some(content())).unwrap();
    let mut control = prepared.control;
    let mut runtime = prepared.runtime;
    let target = prepared.target;
    control
        .submit(Lane::Normal, Frame(0), Op::State { target })
        .map_err(|e| e.error)
        .unwrap();
    control
        .submit(
            Lane::Normal,
            Frame(0),
            Op::Transition {
                target,
                timing: Timing::At,
                transition: Transition::Play {
                    position: 0,
                    retrigger: false,
                },
            },
        )
        .map_err(|e| e.error)
        .unwrap();
    let input = [MidiBuffer::new(1, 3).unwrap()];
    let mut output = [MidiBuffer::new(256, 1024).unwrap()];
    let mut left = [0.; 2];
    let mut right = [0.; 2];
    heap::assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: Frame(0),
                frames: 2,
                inputs: &[&[1., 2.], &[10., 20.]],
                outputs: &mut [&mut left, &mut right],
            },
            MidiBlock {
                inputs: &input,
                outputs: &mut output,
            },
        )
    })
    .unwrap();
    assert_eq!(left, [-9., -18.]);
    assert_eq!(right, [110., 220.]);
    let mut expected = Vec::new();
    for channel in 0..16u8 {
        for packet in [
            [0xE0 | channel, 0, 64],
            [0xB0 | channel, 64, 0],
            [0xB0 | channel, 69, 0],
            [0xB0 | channel, 66, 0],
        ] {
            expected.push((0, packet.to_vec()));
        }
        if channel == 0 {
            expected.push((0, vec![0x90, 62, 40]));
        }
    }
    expected.push((0, vec![0x90, 64, 70]));
    expected.push((1, vec![0x80, 64, 30]));
    let actual = (0..output[0].len())
        .map(|i| {
            let (at, p) = output[0].get(i).unwrap();
            (at, p.to_vec())
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
    while control.try_completion().is_some() {}
    let mut queried = false;
    while let Some(execution) = control.try_execution() {
        let raw = execution;
        assert_eq!(raw.result, Ok(()));
        if let Some(Reply::State(cut)) = raw.operation_result {
            assert_eq!(cut.state.mode, Mode::Playing);
            assert_eq!(cut.state.content_length, 2);
            assert_eq!(cut.state.content_revision, rt_runtime::Revision(0));
            assert_eq!(cut.capture_failure, None);
            queried = true;
        }
    }
    assert!(queried);
    control
        .submit(
            Lane::Recovery,
            runtime.next_frame(),
            Op::Transition {
                target,
                timing: Timing::At,
                transition: Transition::Stop,
            },
        )
        .map_err(|e| e.error)
        .unwrap();
    let mut stopped_left = [0.];
    let mut stopped_right = [0.];
    heap::assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: Frame(2),
                frames: 1,
                inputs: &[&[3.], &[30.]],
                outputs: &mut [&mut stopped_left, &mut stopped_right],
            },
            MidiBlock {
                inputs: &input,
                outputs: &mut output,
            },
        )
    })
    .unwrap();
    assert_eq!(stopped_left, [3.]);
    assert_eq!(output[0].len(), 1);
    assert_eq!(output[0].get(0).unwrap(), (0, &[0x80, 62, 0][..]));
    drop(control);
    let mut stopped = runtime.into_stopped();
    mixed::cleanup(&mut stopped);
    mixed::cleanup(&mut stopped);
    let usage = stopped.resource_usage();
    assert_eq!(usage.free_pages, 128);
    assert_eq!(usage.free_versions, 32);
    assert_eq!(usage.free_snapshots, 32);
    assert_eq!(stopped.resources().midi_index_usage().free_versions, 8);
    let usage = stopped.media_usage();
    assert_eq!(usage.free_media, 8);
    assert_eq!(usage.free_exports, 4);
    assert_eq!(usage.free_pins, 8);
    assert_eq!(usage.references, 0);
}

#[shoop_test]
fn startup_preparation_checks_shapes_packet_capacity_and_multi_page_serialization() {
    let cfg = 128;
    let p = preset();
    let mut bad = content();
    bad.audio[1].pop();
    assert_eq!(
        assemble(cfg, Tap::PostDsp, vec![p], p.id, Some(bad))
            .err()
            .unwrap(),
        Failure::Runtime(Error::InvalidShape)
    );
    let mut bad = content();
    bad.midi[0].bytes = vec![0; 513];
    assert_eq!(
        assemble(cfg, Tap::PostDsp, vec![p], p.id, Some(bad))
            .err()
            .unwrap(),
        Failure::Runtime(Error::Capacity)
    );
    let content = InitialContent {
        audio: [
            (0..70).map(|i| i as f32).collect(),
            (0..70).map(|i| -(i as f32)).collect(),
        ],
        midi_initial: MidiState::default(),
        midi: (0..30)
            .map(|i| InitialMidi {
                frame: i,
                bytes: vec![0xB0, 7, i as u8],
            })
            .collect(),
    };
    let prepared = assemble(cfg, Tap::PostDsp, vec![p], p.id, Some(content)).unwrap();
    drop(prepared.control);
    let mut stopped = prepared.runtime.into_stopped();
    mixed::cleanup(&mut stopped);
    assert_eq!(stopped.resource_usage().free_pages, 128);
    assert_eq!(stopped.resource_usage().free_versions, 32);
    assert_eq!(stopped.resource_usage().free_snapshots, 32);
    assert_eq!(stopped.resources().midi_index_usage().free_versions, 8);
    assert_eq!(stopped.media_usage().references, 0);
}
