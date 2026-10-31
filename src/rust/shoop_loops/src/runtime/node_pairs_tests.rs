use super::*;
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

#[shoop_test]
fn node_pair_and_source_generations_retire_and_failed_validation_keeps_its_owned_snapshot() {
    let settings = NodePairSettings {
        candidates: 1,
        sources: 1,
        source_packets_per_callback: 1,
    };
    assert!(matches!(
        NodePairs::prepare(usize::MAX, settings),
        Err(Error::Capacity)
    ));
    assert!(matches!(
        NodePairs::prepare(
            1,
            NodePairSettings {
                candidates: 0,
                ..settings
            }
        ),
        Err(Error::InvalidConfig)
    ));
    assert!(matches!(
        NodePairs::prepare(
            1,
            NodePairSettings {
                sources: 0,
                ..settings
            }
        ),
        Err(Error::InvalidConfig)
    ));
    assert!(matches!(
        NodePairs::prepare(
            1,
            NodePairSettings {
                source_packets_per_callback: 0,
                ..settings
            }
        ),
        Err(Error::InvalidConfig)
    ));
    assert!(matches!(
        NodePairs::prepare(
            1,
            NodePairSettings {
                source_packets_per_callback: usize::MAX,
                ..settings
            }
        ),
        Err(Error::Capacity)
    ));
    assert_eq!(
        NodePairStageId::from_parts([0, 0, 1]),
        Err(Error::InvalidId)
    );
    assert_eq!(
        NodePairSourceId::from_parts([1, 0, 0]),
        Err(Error::InvalidId)
    );
    let config = rt_runtime::Config {
        resource_slots: 2,
        resource_pages: 8,
        page_bytes: 64,
        ..rt_runtime::Config::default()
    }
    .prepare()
    .unwrap();
    let mut store = ResourceStore::with_midi_indexing(
        config,
        rt_runtime::resource::MidiIndexConfig {
            versions: 1,
            checkpoints_per_version: 1,
            replay_events: 2,
        },
    )
    .unwrap();
    let mut foreign = ResourceStore::new(config).unwrap();
    let audio = store.create().unwrap();
    let midi = store.create().unwrap();
    let token = store
        .begin_upload(
            audio,
            Revision(0),
            rt_runtime::resource::Description {
                format: Format::Audio { channels: 1 },
                frames: 1,
                bytes: 4,
            },
        )
        .unwrap();
    store.begin_callback();
    store.write_upload(token, 0, &1f32.to_le_bytes()).unwrap();
    store.finish_upload(token).unwrap();
    store.commit(token).unwrap();
    let token = store
        .begin_indexed_midi_upload(
            midi,
            Revision(0),
            rt_runtime::resource::Description {
                format: Format::Midi,
                frames: 1,
                bytes: 15,
            },
            &MidiState::default(),
        )
        .unwrap();
    store.begin_callback();
    store
        .write_upload(
            token,
            0,
            &[0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0x90, 60, 100],
        )
        .unwrap();
    store.finish_upload(token).unwrap();
    store.commit(token).unwrap();
    let mut node_pairs = NodePairs::prepare(1, settings).unwrap();
    let mut validation = NodePairs::prepare(1, settings).unwrap();
    node_pairs.banks[0].generation = u32::MAX - 1;
    node_pairs.sources[0].generation = u32::MAX - 1;
    assert_no_heap(|| {
        let id = node_pairs.begin(0, 0).unwrap();
        assert_eq!(id.parts()[2], u32::MAX);
        assert_eq!(node_pairs.begin(0, 0), Err(Error::Busy));
        node_pairs.cancel(id, &mut store.io()).unwrap();
        assert_eq!(node_pairs.begin(0, 0), Err(Error::Exhausted));
        assert_eq!(node_pairs.status(id), Ok(NodePairStatus::Idle));
        let (old, _) = node_pairs
            .snapshot(audio, Frame(0), &mut store.io())
            .unwrap();
        assert_eq!(old.parts()[2], u32::MAX - 1);
        assert_eq!(
            node_pairs.release_source(old, &mut foreign.io()),
            Err(Error::InvalidId)
        );
        assert_eq!(
            node_pairs.source_status(old),
            Ok(NodePairSourceStatus::Ready { max_packet: 0 })
        );
        node_pairs.release_source(old, &mut store.io()).unwrap();
        let (last, _) = node_pairs
            .snapshot(audio, Frame(0), &mut store.io())
            .unwrap();
        assert_eq!(last.parts()[2], u32::MAX);
        assert_eq!(
            node_pairs.release_source(old, &mut store.io()),
            Err(Error::InvalidId)
        );
        node_pairs.release_source(last, &mut store.io()).unwrap();
        assert_eq!(node_pairs.sources[0].generation, 0);
        assert_eq!(
            node_pairs.snapshot(audio, Frame(0), &mut store.io()),
            Err(Error::Capacity)
        );
        let (failed, _) = validation
            .snapshot(midi, Frame(0), &mut store.io())
            .unwrap();
        validation.callback(Frame(0));
        assert_eq!(
            validation.validate_source(failed, &mut foreign.io()),
            Err(Error::InvalidId)
        );
        assert_eq!(
            validation.source_status(failed),
            Ok(NodePairSourceStatus::Failed(Error::InvalidId))
        );
        assert_eq!(
            validation.validate_source(failed, &mut store.io()),
            Err(Error::InvalidId)
        );
        validation.release_source(failed, &mut store.io()).unwrap();
        let (ready, _) = validation
            .snapshot(midi, Frame(1), &mut store.io())
            .unwrap();
        assert_eq!(ready.parts()[2], failed.parts()[2] + 1);
        assert_eq!(
            validation.release_source(failed, &mut store.io()),
            Err(Error::InvalidId)
        );
        validation.callback(Frame(1));
        assert_eq!(
            validation.validate_source(ready, &mut store.io()),
            Ok(NodePairSourceStatus::Validating { packets: 1 })
        );
        validation.callback(Frame(2));
        assert_eq!(
            validation.validate_source(ready, &mut store.io()),
            Ok(NodePairSourceStatus::Ready { max_packet: 3 })
        );
        node_pairs.cleanup(&mut store.io()).unwrap();
        validation.cleanup(&mut store.io()).unwrap();
        validation.cleanup(&mut store.io()).unwrap();
        store.remove(audio).unwrap();
        store.remove(midi).unwrap();
        for _ in 0..8 {
            store.begin_callback();
            store.reclaim();
        }
        assert_eq!(store.usage().free_pages, 8);
        assert_eq!(store.usage().free_snapshots, 2);
    });
}
