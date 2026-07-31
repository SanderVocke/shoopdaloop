#![cfg(feature = "app_backend")]

use shoop_engine::app_backend::{
    AudioDriver, AudioDriverSettings, BackendSession, DummyAudioDriverSettings,
};
use shoop_engine::{
    AudioDriverType, CompositeEntry, CompositePlanDescriptor, CompositeSection, CompositeTimeline,
    LoopMode, LoopTargetMetadata,
};

fn backend() -> (AudioDriver, BackendSession) {
    let driver = AudioDriver::new(AudioDriverType::Dummy, None).unwrap();
    driver
        .start(&AudioDriverSettings::Dummy(DummyAudioDriverSettings {
            client_name: "composite-app-backend-test".to_string(),
            sample_rate: 48_000,
            buffer_size: 4,
        }))
        .unwrap();
    let session = BackendSession::new().unwrap();
    session.set_audio_driver(&driver).unwrap();
    (driver, session)
}

#[test]
fn application_backend_rejects_a_primitive_self_sync_edge() {
    let (_driver, session) = backend();
    let loop_ = session.create_loop().unwrap();

    loop_.set_sync_source(Some(&loop_)).unwrap();

    assert_eq!(session.primitive_sync_sources(), vec![None]);
}

fn descriptor(
    source: shoop_engine::LoopIdentity,
    target: shoop_engine::LoopIdentity,
    sync_length: u64,
) -> CompositePlanDescriptor {
    CompositePlanDescriptor {
        source,
        sync_length,
        timelines: vec![CompositeTimeline {
            sections: vec![CompositeSection {
                entries: vec![CompositeEntry {
                    target,
                    delay: 0,
                    n_cycles: Some(1),
                    mode: None,
                }],
            }],
        }],
    }
}

#[test]
fn application_backend_creates_configures_controls_and_observes_engine_composite() {
    let (_driver, session) = backend();
    let sync = session.create_loop().unwrap();
    let child = session.create_loop().unwrap();
    let composite = session.create_composite_loop().unwrap();
    sync.set_length(4).unwrap();
    child.set_length(4).unwrap();

    let source = composite.identity();
    let sync_identity = sync.identity();
    let child_identity = child.identity();
    let metadata = vec![
        LoopTargetMetadata {
            identity: source,
            length_samples: 0,
        },
        LoopTargetMetadata {
            identity: sync_identity,
            length_samples: 4,
        },
        LoopTargetMetadata {
            identity: child_identity,
            length_samples: 4,
        },
    ];
    let descriptor = CompositePlanDescriptor {
        source,
        sync_length: 4,
        timelines: vec![CompositeTimeline {
            sections: vec![CompositeSection {
                entries: vec![CompositeEntry {
                    target: child_identity,
                    delay: 0,
                    n_cycles: Some(1),
                    mode: None,
                }],
            }],
        }],
    };

    assert_eq!(
        session
            .configure_composite_loop(
                &composite,
                descriptor,
                sync_identity,
                metadata,
                &[None, None],
            )
            .unwrap(),
        1
    );
    sync.transition(LoopMode::Playing, -1, -1).unwrap();
    assert_eq!(
        composite
            .transition_immediate(LoopMode::Playing, 0)
            .unwrap(),
        0
    );
    assert_eq!(composite.set_play_after_record(true).unwrap(), 1);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let state = loop {
        let state = composite.get_state().unwrap();
        if state.mode == LoopMode::Playing || std::time::Instant::now() >= deadline {
            break state;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    assert_eq!(state.identity, source);
    assert_eq!(state.active_plan_version, 1);
    assert_eq!(state.mode, LoopMode::Playing);
    assert_eq!(state.length, 4);
    assert!(state.play_after_record);
    assert_eq!(state.active_children.len(), 1);
    assert_eq!(state.active_children[0].identity, child_identity);

    assert_eq!(
        session
            .remove_composite_loop(&composite, &[None, None])
            .unwrap(),
        2
    );
    assert!(composite.get_state().is_err());
    assert_eq!(child.get_state().unwrap().mode, LoopMode::Stopped);
}

#[test]
fn application_composite_registry_rejects_a_cycle_transactionally() {
    let (_driver, session) = backend();
    let sync = session.create_loop().unwrap();
    sync.set_length(8).unwrap();
    let sync_identity = sync.identity();
    let first = session.create_composite_loop().unwrap();
    let second = session.create_composite_loop().unwrap();
    let metadata = |identity, length_samples| LoopTargetMetadata {
        identity,
        length_samples,
    };

    let empty_second = CompositePlanDescriptor {
        source: second.identity(),
        sync_length: 8,
        timelines: Vec::new(),
    };
    assert_eq!(
        session
            .configure_composite_loop(
                &second,
                empty_second,
                sync_identity,
                vec![metadata(second.identity(), 0), metadata(sync_identity, 8),],
                &[None],
            )
            .unwrap(),
        1
    );
    assert_eq!(
        session
            .configure_composite_loop(
                &first,
                descriptor(first.identity(), second.identity(), 8),
                sync_identity,
                vec![
                    metadata(first.identity(), 8),
                    metadata(second.identity(), 8),
                    metadata(sync_identity, 8),
                ],
                &[None],
            )
            .unwrap(),
        2
    );

    let error = session
        .configure_composite_loop(
            &second,
            descriptor(second.identity(), first.identity(), 8),
            sync_identity,
            vec![
                metadata(first.identity(), 8),
                metadata(second.identity(), 8),
                metadata(sync_identity, 8),
            ],
            &[None],
        )
        .unwrap_err();
    assert!(error.to_string().contains("cycle"), "{error:#}");
    assert_eq!(first.get_state().unwrap().active_plan_version, 2);
    assert_eq!(second.get_state().unwrap().active_plan_version, 2);

    first.transition_immediate(LoopMode::Playing, 0).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let first_state = first.get_state().unwrap();
        if first_state.mode == LoopMode::Playing || std::time::Instant::now() >= deadline {
            assert_eq!(first_state.mode, LoopMode::Playing);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    assert_eq!(session.remove_composite_loop(&second, &[None]).unwrap(), 3);
    assert!(first.get_state().is_err());
    assert!(second.get_state().is_err());
    assert_eq!(session.remove_composite_loop(&first, &[None]).unwrap(), 0);
}
