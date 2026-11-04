use shoop_port_graph::{testing::*, *};
use shoop_wasm_test_support::shoop_test;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn spec(name: &str, data_type: DataType, direction: Direction) -> PortSpec {
    PortSpec {
        name: name.into(),
        data_type,
        direction,
    }
}
fn publish(fake: &mut FakeProvider, control: &mut Control, key: LocalPortKey) -> EndpointId {
    let outcome = perform(
        control,
        Operation::Publish {
            key,
            spec: spec("duplicate", DataType::Audio, Direction::Output),
        },
        || fake.advance(),
    )
    .unwrap();
    let Outcome::Published { port, .. } = outcome else {
        panic!()
    };
    port
}
#[shoop_test]
fn fake_runs_shared_connection_contract_without_name_identity_or_jack_namespaces() {
    let mut fake = FakeProvider::new(42);
    let mut control = fake.control();
    let local = publish(&mut fake, &mut control, LocalPortKey(1));
    let peer = fake.peer(spec("duplicate", DataType::Audio, Direction::Input));
    let incompatible = fake.peer(spec("duplicate", DataType::Midi, Direction::Input));
    let foreign = fake.peer(spec("duplicate", DataType::Audio, Direction::Output));
    assert_eq!(
        control
            .snapshot()
            .ports
            .iter()
            .filter(|p| p.name == "duplicate")
            .count(),
        4
    );
    connection_contract(&mut control, local, peer, incompatible, foreign, || {
        fake.advance()
    });
}
#[shoop_test]
fn publication_is_observed_and_idempotent_with_failure_removal_and_new_incarnations() {
    let mut fake = FakeProvider::new(7);
    let mut control = fake.control();
    let key = LocalPortKey(1);
    let request = Request {
        epoch: 7,
        operation: Operation::Publish {
            key,
            spec: spec("duplicate", DataType::Audio, Direction::Output),
        },
    };
    let id = control.submit(request.clone()).unwrap();
    assert!(control.snapshot().publications.is_empty());
    fake.advance();
    assert!(matches!(
        control.snapshot().publication(key).unwrap().state,
        PublicationState::Publishing
    ));
    assert!(control.take_result().is_none());
    fake.advance();
    let Outcome::Published { port, .. } = control.take_result().unwrap().result.unwrap() else {
        panic!()
    };
    assert_eq!(id, OperationId(1));
    let repeat = control.submit(request.clone()).unwrap();
    assert_eq!(
        finish(&mut control, repeat, || fake.advance()).unwrap(),
        Outcome::Published { key, port }
    );
    let peer = fake.peer(spec("duplicate", DataType::Audio, Direction::Input));
    perform(
        &mut control,
        Operation::Connect {
            local: port,
            peer,
            connected: true,
        },
        || fake.advance(),
    )
    .unwrap();
    let removal = control
        .submit(Request {
            epoch: 7,
            operation: Operation::Unpublish { key },
        })
        .unwrap();
    fake.advance();
    assert!(
        matches!(control.snapshot().publication(key).unwrap().state, PublicationState::Removing(id) if id == port)
    );
    assert!(control.snapshot().connected(port, peer));
    finish(&mut control, removal, || fake.advance()).unwrap();
    assert!(control.snapshot().port(port).is_none());
    assert!(control.snapshot().links.is_empty());
    assert!(control.snapshot().publication(key).is_none());
    let republished = publish(&mut fake, &mut control, key);
    assert_ne!(republished, port);
    assert_eq!(
        control
            .submit(Request {
                epoch: 7,
                operation: Operation::Connect {
                    local: port,
                    peer,
                    connected: true
                }
            })
            .unwrap_err()
            .kind,
        ErrorKind::StalePort
    );
    let failed = LocalPortKey(2);
    fake.fail_next(Error::new(
        ErrorKind::Backend,
        "provider registration failed",
    ));
    assert_eq!(
        perform(
            &mut control,
            Operation::Publish {
                key: failed,
                spec: spec("failed", DataType::Midi, Direction::Input)
            },
            || fake.advance()
        )
        .unwrap_err()
        .kind,
        ErrorKind::Backend
    );
    assert!(matches!(
        control.snapshot().publication(failed).unwrap().state,
        PublicationState::Failed(_)
    ));
    assert!(control.snapshot().valid());
    fake.stop();
    assert_eq!(control.snapshot().status, Status::Unavailable);
    assert_eq!(
        control.submit(request).unwrap_err().kind,
        ErrorKind::Unavailable
    );
}
#[shoop_test]
fn unrelated_changes_and_renaming_do_not_invalidate_ids_but_same_label_replacement_does() {
    let mut fake = FakeProvider::new(7);
    let mut control = fake.control();
    let local = publish(&mut fake, &mut control, LocalPortKey(1));
    let peer = fake.peer(spec("before", DataType::Audio, Direction::Input));
    let request = Request {
        epoch: 7,
        operation: Operation::Connect {
            local,
            peer,
            connected: true,
        },
    };
    fake.rename_peer(peer, "after");
    fake.peer(spec("unrelated", DataType::Midi, Direction::Input));
    let id = control.submit(request.clone()).unwrap();
    finish(&mut control, id, || fake.advance()).unwrap();
    fake.remove_peer(peer);
    let replacement = fake.peer(spec("after", DataType::Audio, Direction::Input));
    assert_ne!(replacement, peer);
    assert_eq!(
        control.submit(request).unwrap_err().kind,
        ErrorKind::StalePort
    );
}
#[shoop_test]
fn channel_bounds_inflight_work_and_worker_drop_refuses_even_dequeued_operations() {
    let mut fake = FakeProvider::new(7);
    let mut control = fake.control();
    let local = publish(&mut fake, &mut control, LocalPortKey(1));
    let peer = fake.peer(spec("peer", DataType::Audio, Direction::Input));
    let request = Request {
        epoch: 7,
        operation: Operation::Connect {
            local,
            peer,
            connected: true,
        },
    };
    for _ in 0..MAX_PENDING {
        control.submit(request.clone()).unwrap();
    }
    fake.advance();
    assert_eq!(control.submit(request).unwrap_err().kind, ErrorKind::Busy);
    drop(fake);
    let mut count = 0;
    while let Some(completion) = control.take_result() {
        assert_eq!(completion.result.unwrap_err().kind, ErrorKind::Unavailable);
        count += 1;
    }
    assert_eq!(count, MAX_PENDING);
}

#[shoop_test]
fn invalid_snapshots_are_refused_and_success_cannot_precede_observation() {
    let mut fake = FakeProvider::new(7);
    let mut control = fake.control();
    let local = publish(&mut fake, &mut control, LocalPortKey(1));
    let peer = fake.peer(spec("peer", DataType::Audio, Direction::Input));
    let snapshot = (*control.snapshot()).clone();
    assert!(snapshot.valid());
    let (other, mut worker) = channel(7, "fake", snapshot.capabilities).unwrap();
    worker.publish(snapshot.clone()).unwrap();
    let mut bad = snapshot.clone();
    bad.publications.clear();
    assert_eq!(worker.publish(bad).unwrap_err().kind, ErrorKind::Invalid);
    assert_eq!(other.snapshot().ports, snapshot.ports);
    let mut bad = snapshot.clone();
    bad.ports.push(bad.ports[0].clone());
    assert!(!bad.valid());
    let mut bad = snapshot.clone();
    bad.links.push(Link {
        source: local,
        destination: EndpointId(9999),
    });
    assert!(!bad.valid());
    let mut bad = snapshot.clone();
    bad.ports[0].locator = Some("x".repeat(257));
    assert!(!bad.valid());
    let mut bad = snapshot.clone();
    bad.publications[0].key = LocalPortKey(0);
    assert!(!bad.valid());
    let id = other
        .submit(Request {
            epoch: 7,
            operation: Operation::Connect {
                local,
                peer,
                connected: true,
            },
        })
        .unwrap();
    assert_eq!(worker.next_request().unwrap().0, id);
    worker.complete(id, Ok(Outcome::Connected));
    assert_eq!(
        other.take_result().unwrap().result.unwrap_err().kind,
        ErrorKind::Backend
    );
    assert!(!other.snapshot().connected(local, peer));
}
#[shoop_test]
fn failed_removal_retains_the_published_port_and_can_be_retried() {
    let mut fake = FakeProvider::new(7);
    let mut control = fake.control();
    let key = LocalPortKey(1);
    let port = publish(&mut fake, &mut control, key);
    fake.fail_next(Error::new(ErrorKind::Backend, "device rejected removal"));
    assert_eq!(
        perform(&mut control, Operation::Unpublish { key }, || fake
            .advance())
        .unwrap_err()
        .kind,
        ErrorKind::Backend
    );
    assert_eq!(
        control.snapshot().publication(key).unwrap().state,
        PublicationState::Published(port)
    );
    assert!(control.snapshot().port(port).is_some());
    perform(&mut control, Operation::Unpublish { key }, || {
        fake.advance()
    })
    .unwrap();
    assert!(control.snapshot().port(port).is_none());
}
