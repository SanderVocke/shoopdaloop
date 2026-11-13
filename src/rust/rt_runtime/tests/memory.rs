use rt_runtime::{
    resource::{shared_audio, Description, Format, ResourceStore},
    Config, Frame, Quantum, Revision,
};
use shoop_wasm_test_support::{
    heap::{assert_no_heap, measure_memory, TrackingAllocator},
    shoop_test,
};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[shoop_test]
fn accounted_storage_matches_allocations_including_prepared_audio_reserves() {
    for audio in [false, true] {
        let (store, memory) = measure_memory(|| {
            let mut store = ResourceStore::new(
                Config::default().prepare().unwrap(),
                Quantum::new(1).unwrap(),
            )
            .unwrap();
            if audio {
                store
                    .prepare_audio_store(
                        shared_audio::Config {
                            channels: 2,
                            history_frames: 128,
                            views: 4,
                            pages: 64,
                        },
                        Frame(0),
                    )
                    .unwrap();
            }
            store
        });
        assert_no_heap(|| {
            let usage = store.memory_usage();
            assert_eq!(usage.accounted_bytes, memory.live as u64);
            assert_eq!(
                usage.payload_bytes,
                Config::default().resource_pages as u64 * 4096
            );
            assert_eq!(usage.pools.reserved_pages > 0, audio);
        });
    }
}

#[shoop_test]
fn snapshots_share_payload_and_keep_it_committed_until_reclamation() {
    let mut store = ResourceStore::new(
        Config::default().prepare().unwrap(),
        Quantum::new(1).unwrap(),
    )
    .unwrap();
    let baseline = store.memory_usage();
    let id = store.create().unwrap();
    let upload = store
        .begin_upload(
            id,
            Revision(0),
            Description {
                format: Format::Audio { channels: 1 },
                frames: 4,
                bytes: 16,
            },
        )
        .unwrap();
    store.begin_callback();
    store.write_upload(upload, 0, &[0; 16]).unwrap();
    store.finish_upload(upload).unwrap();
    store.commit(upload).unwrap();
    assert_no_heap(|| {
        let first = store.snapshot(id, Frame(0)).unwrap();
        let second = store.snapshot(id, Frame(0)).unwrap();
        let retained = store.memory_usage();
        assert_eq!(retained.accounted_bytes, baseline.accounted_bytes);
        assert_eq!(retained.pools.free_pages, baseline.pools.free_pages - 1);
        assert_eq!(
            retained.pools.free_snapshots,
            baseline.pools.free_snapshots - 2
        );
        store.remove(id).unwrap();
        store.release_snapshot(first.id).unwrap();
        assert_eq!(
            store.memory_usage().pools.free_pages,
            retained.pools.free_pages
        );
        store.release_snapshot(second.id).unwrap();
        for _ in 0..8 {
            store.begin_callback();
            store.reclaim();
        }
        assert_eq!(
            store.memory_usage().pools.free_pages,
            baseline.pools.free_pages
        );
        assert_eq!(
            store.memory_usage().accounted_bytes,
            baseline.accounted_bytes
        );
    });
}

#[shoop_test]
fn midi_storage_accounting_includes_state_and_transaction_workspace() {
    use rt_runtime::resource::shared_midi;
    let (store, memory) = measure_memory(|| {
        let mut store = ResourceStore::new(
            Config {
                resource_pages: 1024,
                ..Config::default()
            }
            .prepare()
            .unwrap(),
            Quantum::new(1).unwrap(),
        )
        .unwrap();
        store
            .prepare_midi_store(
                shared_midi::Config {
                    history_frames: 16,
                    chunks: 8,
                    views: 8,
                    pages: 1000,
                    max_events: 32,
                    max_bytes: 256,
                    max_packet: 48,
                    transactions: 2,
                    edit_steps: 3,
                },
                Frame(0),
                &rt_runtime::midi_state::MidiState::default(),
            )
            .unwrap();
        store
    });
    assert_eq!(store.memory_usage().accounted_bytes, memory.live as u64);
}

#[shoop_test]
fn indexed_midi_accounting_includes_shared_packet_and_index_pools() {
    use rt_runtime::resource::indexed_midi::*;
    let (store, memory) = measure_memory(|| {
        let mut store = ResourceStore::new(
            rt_runtime::Config {
                max_frames: 32,
                resource_pages: 256,
                page_bytes: 64,
                ..Default::default()
            }
            .prepare()
            .unwrap(),
            Quantum::new(32).unwrap(),
        )
        .unwrap();
        store
            .prepare_indexed_midi_store(Config {
                packets: PacketLimits {
                    pages: 128,
                    bodies: 512,
                    records: 2048,
                    max_packet: 64,
                    bases: 4,
                },
                indexes: IndexLimits {
                    leases: 32,
                    versions: 16,
                    category_pages: 256,
                    packet_nodes: 4096,
                    scalar_nodes: 4096,
                    hold_nodes: 1024,
                    sostenuto_nodes: 1024,
                },
                sources: 2,
                history_frames: 128,
                history_events: 32,
                history_bytes: 1024,
                views: 8,
                writers: 4,
                imports: 2,
                events_per_tick: 8,
                bytes_per_tick: 128,
                maintenance_steps_per_tick: 32,
            })
            .unwrap();
        store
    });
    assert_eq!(store.memory_usage().accounted_bytes, memory.live as u64);
}
