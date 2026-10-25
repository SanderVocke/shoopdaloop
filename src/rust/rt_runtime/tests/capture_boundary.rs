mod support;
use rt_runtime::{resource::*, Config, Error, Frame};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
#[shoop_test]
fn planner_capture_authority_is_bound_to_the_runtime_boundary_and_ceiling() {
    let config = Config {
        max_frames: 4,
        resource_slots: 4,
        resource_pages: 8,
        page_bytes: 16,
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let mut store = ResourceStore::with_capture_groups(
        config,
        None,
        CaptureGroupConfig {
            groups: 1,
            members_per_group: 1,
            staging_bytes_per_group: 16,
        },
    )
    .unwrap();
    assert_no_heap(|| {
        let resource = store.create().unwrap();
        let group = store
            .bind_capture_group(&[CaptureMember {
                resource,
                format: Format::Audio { channels: 1 },
                midi_events: 0,
                midi_bytes: 0,
            }])
            .unwrap();
        let take = store
            .planner_io(Frame(10), Frame(12))
            .capture()
            .prepare(group, &[])
            .unwrap();
        assert_eq!(
            store
                .planner_io(Frame(10), Frame(12))
                .capture()
                .activate(take, Frame(9)),
            Err(Error::InvalidFrame)
        );
        store
            .planner_io(Frame(10), Frame(12))
            .capture()
            .activate(take, Frame(10))
            .unwrap();
        let baseline = store.usage();
        assert_eq!(
            store
                .planner_io(Frame(10), Frame(12))
                .capture()
                .reserve(take, Frame(10), 3),
            Err(Error::InvalidFrame)
        );
        assert_eq!(
            store
                .planner_io(Frame(10), Frame(12))
                .capture()
                .reserve(take, Frame(11), 1),
            Err(Error::InvalidFrame)
        );
        assert_eq!(store.usage(), baseline);
        let interval = store
            .planner_io(Frame(10), Frame(12))
            .capture()
            .reserve(take, Frame(10), 2)
            .unwrap();
        store
            .io()
            .render()
            .stage_audio(interval, 0, &[1., 2.])
            .unwrap();
        assert_eq!(
            store
                .planner_io(Frame(11), Frame(12))
                .capture()
                .commit(interval),
            Err(Error::InvalidFrame)
        );
        assert_eq!(store.info(resource).unwrap().description.frames, 0);
        store
            .planner_io(Frame(12), Frame(12))
            .capture()
            .commit(interval)
            .unwrap();
        let mut output = [None];
        assert_eq!(
            store.planner_io(Frame(11), Frame(12)).capture().freeze(
                take,
                Frame(12),
                None,
                &mut output
            ),
            Err(Error::InvalidFrame)
        );
        store
            .planner_io(Frame(12), Frame(12))
            .capture()
            .freeze(take, Frame(12), None, &mut output)
            .unwrap();
        assert_eq!(output[0].unwrap().description.frames, 2);
        store.release_snapshot(output[0].unwrap().id).unwrap();
        let next = store.prepare_capture_take(group, &[]).unwrap();
        store.activate_capture_take(next, Frame(u64::MAX)).unwrap();
        assert_eq!(
            store.reserve_capture_interval(next, Frame(u64::MAX), 1),
            Err(Error::Exhausted)
        );
        let mut output = [None];
        store
            .freeze_capture_take(next, Frame(u64::MAX), None, &mut output)
            .unwrap();
        store.release_snapshot(output[0].unwrap().id).unwrap();
        store.release_capture_group(group).unwrap();
        store.remove(resource).unwrap();
        store.begin_callback();
        store.reclaim();
        assert_eq!(store.usage().free_versions, 4);
        assert_eq!(store.usage().free_snapshots, 4);
        assert_eq!(store.usage().free_pages, 8);
    });
}
