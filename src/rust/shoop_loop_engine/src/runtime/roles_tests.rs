use super::*;
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

#[shoop_test]
fn role_and_source_generations_retire_and_failed_validation_keeps_its_owned_snapshot() {
    let settings = RoleSettings {
        candidates: 1,
        sources: 1,
        source_packets_per_callback: 1,
    };
    assert!(matches!(
        Roles::prepare(usize::MAX, settings),
        Err(Error::Capacity)
    ));
    assert!(matches!(
        Roles::prepare(
            1,
            RoleSettings {
                candidates: 0,
                ..settings
            }
        ),
        Err(Error::InvalidConfig)
    ));
    assert!(matches!(
        Roles::prepare(
            1,
            RoleSettings {
                sources: 0,
                ..settings
            }
        ),
        Err(Error::InvalidConfig)
    ));
    assert!(matches!(
        Roles::prepare(
            1,
            RoleSettings {
                source_packets_per_callback: 0,
                ..settings
            }
        ),
        Err(Error::InvalidConfig)
    ));
    assert!(matches!(
        Roles::prepare(
            1,
            RoleSettings {
                source_packets_per_callback: usize::MAX,
                ..settings
            }
        ),
        Err(Error::Capacity)
    ));
    assert_eq!(RoleStageId::from_parts([0, 0, 1]), Err(Error::InvalidId));
    assert_eq!(RoleSourceId::from_parts([1, 0, 0]), Err(Error::InvalidId));
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
    let mut roles = Roles::prepare(1, settings).unwrap();
    let mut validation = Roles::prepare(1, settings).unwrap();
    roles.banks[0].generation = u32::MAX - 1;
    roles.sources[0].generation = u32::MAX - 1;
    assert_no_heap(|| {
        let id = roles.begin(0, 0).unwrap();
        assert_eq!(id.parts()[2], u32::MAX);
        assert_eq!(roles.begin(0, 0), Err(Error::Busy));
        roles.cancel(id, &mut store.io()).unwrap();
        assert_eq!(roles.begin(0, 0), Err(Error::Exhausted));
        assert_eq!(roles.status(id), Ok(RoleStatus::Idle));
        let (old, _) = roles.snapshot(audio, Frame(0), &mut store.io()).unwrap();
        assert_eq!(old.parts()[2], u32::MAX - 1);
        assert_eq!(
            roles.release_source(old, &mut foreign.io()),
            Err(Error::InvalidId)
        );
        assert_eq!(
            roles.source_status(old),
            Ok(RoleSourceStatus::Ready { max_packet: 0 })
        );
        roles.release_source(old, &mut store.io()).unwrap();
        let (last, _) = roles.snapshot(audio, Frame(0), &mut store.io()).unwrap();
        assert_eq!(last.parts()[2], u32::MAX);
        assert_eq!(
            roles.release_source(old, &mut store.io()),
            Err(Error::InvalidId)
        );
        roles.release_source(last, &mut store.io()).unwrap();
        assert_eq!(roles.sources[0].generation, 0);
        assert_eq!(
            roles.snapshot(audio, Frame(0), &mut store.io()),
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
            Ok(RoleSourceStatus::Failed(Error::InvalidId))
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
            Ok(RoleSourceStatus::Validating { packets: 1 })
        );
        validation.callback(Frame(2));
        assert_eq!(
            validation.validate_source(ready, &mut store.io()),
            Ok(RoleSourceStatus::Ready { max_packet: 3 })
        );
        roles.cleanup(&mut store.io()).unwrap();
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
