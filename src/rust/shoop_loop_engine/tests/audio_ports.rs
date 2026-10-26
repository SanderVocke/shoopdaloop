#[path = "../../rt_runtime/tests/support/mod.rs"]
mod support;
use rt_runtime::{buffer::AudioPorts, resource::*, Budgets, Config, Error, Frame, Revision};
use shoop_loop_engine::audio::{AudioCapture, AudioRenderer, Playback, Smoothing};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
fn store() -> ResourceStore {
    ResourceStore::with_capture_groups(
        Config {
            max_frames: 8,
            resource_slots: 8,
            resource_pages: 64,
            page_bytes: 16,
            budgets: Budgets {
                transfer_bytes: 16,
                validation_steps: 128,
                ..Budgets::default()
            },
            ..Config::default()
        }
        .prepare()
        .unwrap(),
        None,
        CaptureGroupConfig {
            groups: 1,
            members_per_group: 1,
            staging_bytes_per_group: 64,
        },
    )
    .unwrap()
}
fn cleanup(store: &mut ResourceStore, snapshot: Snapshot) {
    assert_no_heap(|| {
        store.release_snapshot(snapshot.id).unwrap();
        store.remove(snapshot.resource).unwrap();
        for _ in 0..80 {
            store.begin_callback();
            store.reclaim();
        }
        assert_eq!(store.usage().free_pages, 64);
        assert_eq!(store.usage().free_versions, 8);
        assert_eq!(store.usage().free_snapshots, 8);
    });
}
#[shoop_test]
fn runtime_output_ports_match_scalar_ramps_and_slices_without_touching_surrounding_frames() {
    let mut store = store();
    let resource = store.create().unwrap();
    let data: Vec<u8> = (0..6)
        .flat_map(|frame| (0..5).map(move |channel| (frame * 10 + channel + 1) as f32))
        .flat_map(f32::to_le_bytes)
        .collect();
    let upload = store
        .begin_upload(
            resource,
            Revision(0),
            Description {
                format: Format::Audio { channels: 5 },
                frames: 6,
                bytes: data.len(),
            },
        )
        .unwrap();
    for (page, chunk) in data.chunks(16).enumerate() {
        store.begin_callback();
        store.write_upload(upload, page * 16, chunk).unwrap();
    }
    store.finish_upload(upload).unwrap();
    store.commit(upload).unwrap();
    let snapshot = store.snapshot(resource, Frame(0)).unwrap();
    let index = store.read_index(snapshot.id).unwrap();
    let mut ports_renderer =
        AudioRenderer::prepare(5, 4, Smoothing::LinearRestart { frames: 3 }).unwrap();
    let mut slices_renderer =
        AudioRenderer::prepare(5, 4, Smoothing::LinearRestart { frames: 3 }).unwrap();
    let input = [777.; 8];
    let mut backing = [91.; 40];
    let mut planar = [[0.; 2]; 5];
    assert_no_heap(|| {
        for cut in 0..2 {
            backing.fill(91.);
            let playback = Playback {
                source: Some(index),
                position: 1 + 2 * cut,
                gain: 0.5,
                discontinuity: cut == 0,
            };
            if cut == 1 {
                let mut wrong = AudioPorts::new(&input, &mut backing[..32], 8, 3, 2).unwrap();
                assert_eq!(
                    ports_renderer.render_ports(&mut store.render_io(), playback, &mut wrong),
                    Err(Error::InvalidShape)
                );
                let mut ports = AudioPorts::new(&input, &mut backing, 8, 3, 2).unwrap();
                assert_eq!(
                    ports_renderer.render_ports(
                        &mut store.render_io(),
                        Playback {
                            position: 6,
                            discontinuity: true,
                            ..playback
                        },
                        &mut ports
                    ),
                    Err(Error::InvalidFrame)
                );
                let mut empty = AudioPorts::new(&input, &mut backing, 8, 3, 0).unwrap();
                ports_renderer
                    .render_ports(
                        &mut store.render_io(),
                        Playback {
                            source: None,
                            position: 0,
                            gain: f32::NAN,
                            discontinuity: true,
                        },
                        &mut empty,
                    )
                    .unwrap();
                assert_eq!(backing, [91.; 40]);
            }
            let mut ports = AudioPorts::new(&input, &mut backing, 8, 3, 2).unwrap();
            ports_renderer
                .render_ports(&mut store.render_io(), playback, &mut ports)
                .unwrap();
            let [a, b, c, d, e] = &mut planar;
            slices_renderer
                .render(&mut store.render_io(), playback, &mut [a, b, c, d, e])
                .unwrap();
            for channel in 0..5 {
                for frame in 0..8 {
                    let actual = backing[channel * 8 + frame];
                    if (3..5).contains(&frame) {
                        let local = frame - 3;
                        let ordinal = cut as usize * 2 + local;
                        let raw = ((ordinal + 1) * 10 + channel + 1) as f32 * 0.5;
                        let expected = raw * ((ordinal + 1).min(3) as f32 / 3.);
                        assert!((actual - expected).abs() < 1e-5);
                        assert_eq!(actual, planar[channel][local]);
                    } else {
                        assert_eq!(actual, 91.);
                    }
                }
            }
        }
        let mut ports = AudioPorts::new(&input, &mut backing, 8, 3, 2).unwrap();
        ports_renderer
            .render_ports(
                &mut store.render_io(),
                Playback {
                    source: None,
                    position: 0,
                    gain: 1.,
                    discontinuity: false,
                },
                &mut ports,
            )
            .unwrap();
        for channel in 0..5 {
            for frame in 0..8 {
                assert_eq!(
                    backing[channel * 8 + frame],
                    if (3..5).contains(&frame) { 0. } else { 91. }
                );
            }
        }
    });
    assert_eq!(input, [777.; 8]);
    cleanup(&mut store, snapshot);
}
#[shoop_test]
fn runtime_input_ports_stage_exact_stereo_window_and_keep_failed_contributions_unpublished() {
    let mut store = store();
    let resource = store.create().unwrap();
    let group = store
        .bind_capture_group(&[CaptureMember {
            resource,
            format: Format::Audio { channels: 2 },
            midi_events: 0,
            midi_bytes: 0,
        }])
        .unwrap();
    let mut capture = AudioCapture::prepare(2, 8).unwrap();
    let mut input = [0.; 16];
    for channel in 0..2 {
        for frame in 0..8 {
            input[channel * 8 + frame] = (channel * 10 + frame) as f32;
        }
    }
    let original = input;
    let mut output = [80.; 8];
    let mut frozen = [None];
    assert_no_heap(|| {
        let take = store.prepare_capture_take(group, &[]).unwrap();
        let interval = store
            .reserve_prepared_capture_interval(take, Frame(10), 3)
            .unwrap();
        let ports = AudioPorts::new(&input, &mut output, 8, 2, 3).unwrap();
        assert_eq!(
            capture.stage_ports(&mut store.render_io(), interval, 0, &ports),
            Err(Error::InvalidState)
        );
        store.activate_capture_take(take, Frame(10)).unwrap();
        let wrong_channels = AudioPorts::new(&input[..8], &mut output, 8, 2, 3).unwrap();
        assert_eq!(
            capture.stage_ports(&mut store.render_io(), interval, 0, &wrong_channels),
            Err(Error::InvalidShape)
        );
        let wrong_frames = AudioPorts::new(&input, &mut output, 8, 2, 2).unwrap();
        assert_eq!(
            capture.stage_ports(&mut store.render_io(), interval, 0, &wrong_frames),
            Err(Error::InvalidShape)
        );
        let empty = AudioPorts::new(&input, &mut output, 8, 2, 0).unwrap();
        assert_eq!(
            capture.stage_ports(&mut store.render_io(), interval, 0, &empty),
            Err(Error::InvalidFrame)
        );
        assert_eq!(store.info(resource).unwrap().description.frames, 0);
        let ports = AudioPorts::new(&input, &mut output, 8, 2, 3).unwrap();
        capture
            .stage_ports(&mut store.render_io(), interval, 0, &ports)
            .unwrap();
        assert_eq!(store.info(resource).unwrap().description.frames, 0);
        store.commit_capture_interval(interval).unwrap();
        store
            .freeze_capture_take(take, Frame(13), None, &mut frozen)
            .unwrap();
        let snapshot = frozen[0].unwrap();
        assert_eq!(snapshot.description.frames, 3);
        assert_eq!(snapshot.description.bytes, 24);
        let index = store.read_index(snapshot.id).unwrap();
        let mut cursor = store.render_io().cursor_at(index, 0).unwrap();
        let mut bytes = [0; 24];
        let mut copied = 0;
        while copied < bytes.len() {
            let count = store
                .render_io()
                .read(&mut cursor, &mut bytes[copied..])
                .unwrap();
            assert!(count > 0);
            copied += count;
        }
        for (sample, chunk) in [2f32, 12., 3., 13., 4., 14.]
            .iter()
            .zip(bytes.chunks_exact(4))
        {
            assert_eq!(chunk, sample.to_le_bytes());
        }
        store.release_capture_group(group).unwrap();
    });
    assert_eq!(input, original);
    assert_eq!(output, [80.; 8]);
    cleanup(&mut store, frozen[0].unwrap());
}
