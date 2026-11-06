use profiling_protocol::*;
use shoop_wasm_test_support::shoop_test;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
fn stream() -> StreamId {
    StreamId {
        source: 17,
        epoch: 5,
        report_type: 0xfffffff0,
        schema: 444,
    }
}
#[shoop_test]
fn opaque_unknown_reports_roundtrip_and_assemble_without_schema_knowledge() {
    let data = vec![91; CHUNK_BYTES * 2 + 17];
    let mut transfer = Transfer::new(stream(), 12, data.clone()).unwrap();
    let mut assembly = Assembly::default();
    let mut output = [0; MAX_RECORD_BYTES];
    let mut complete = None;
    while let Some(chunk) = transfer.next_chunk() {
        let record = Record::Data(chunk);
        let len = encode(&record, &mut output).unwrap();
        let decoded: Record = decode(&output[..len]).unwrap();
        assert_eq!(record, decoded);
        let Record::Data(decoded) = decoded else {
            panic!("data");
        };
        complete = assembly
            .accept(&decoded)
            .unwrap()
            .map(|bytes| bytes.to_vec());
    }
    assert_eq!(complete, Some(data));
    assert!(transfer.next_chunk().is_none());
}
#[shoop_test]
fn assembly_rejects_gaps_duplicates_stale_epochs_and_oversize_without_reserving_total() {
    let mut transfer = Transfer::new(stream(), 12, vec![1; CHUNK_BYTES + 1]).unwrap();
    let first = transfer.next_chunk().unwrap();
    let last = transfer.next_chunk().unwrap();
    let mut assembly = Assembly::default();
    assert_eq!(assembly.accept(&last), Err(Error::Gap));
    assert!(assembly.accept(&first).unwrap().is_none());
    assert_eq!(assembly.bytes(), CHUNK_BYTES);
    assert!(assembly.accept(&last).unwrap().is_some());
    assert_eq!(assembly.accept(&last), Err(Error::Stale));
    assert_eq!(assembly.accept(&first), Err(Error::Stale));
    let mut stale = first;
    stale.stream.epoch -= 1;
    assert_eq!(assembly.accept(&stale), Err(Error::Stale));
    let mut invalid = first;
    invalid.total = MAX_REPORT_BYTES as u32 + 1;
    assert_eq!(assembly.accept(&invalid), Err(Error::Invalid));
    invalid = first;
    invalid.offset = u32::MAX;
    assert_eq!(assembly.accept(&invalid), Err(Error::Invalid));
}
#[shoop_test]
fn abandoned_report_counts_loss_and_control_payloads_are_bounded() {
    let mut first = Transfer::new(stream(), 1, vec![1; CHUNK_BYTES + 1]).unwrap();
    let mut next = Transfer::new(stream(), 2, vec![2; 13]).unwrap();
    let mut assembly = Assembly::default();
    assembly.accept(&first.next_chunk().unwrap()).unwrap();
    assert_eq!(
        assembly.accept(&next.next_chunk().unwrap()).unwrap(),
        Some(&[2; 13][..])
    );
    assert_eq!(assembly.lost, 1);
    assert!(Bytes::<MAX_CONTROL_BYTES>::new(&[0; MAX_CONTROL_BYTES + 1]).is_none());
    assert!(!Request {
        source: 1,
        epoch: 1,
        action: Action::Subscribe {
            enabled: true,
            interval_ms: 0
        }
    }
    .valid());
}

#[shoop_test]
fn broker_rotates_sources_preserves_active_reports_and_counts_coalescing() {
    use profiling_protocol::broker::Broker;
    let descriptor = |source| Descriptor {
        stream: StreamId { source, ..stream() },
        name: Name::new("test.opaque").unwrap(),
        available: true,
        min_interval_ms: 1,
        max_interval_ms: 1000,
        snapshot: true,
        inspect: false,
    };
    let mut broker = Broker::default();
    broker.register(descriptor(1), None).unwrap();
    broker.register(descriptor(2), None).unwrap();
    broker.publish(1, vec![1; CHUNK_BYTES * 5]).unwrap();
    broker.publish(2, vec![2; 1]).unwrap();
    let mut records = Vec::new();
    while let Some(record) = broker.next_record() {
        records.push(record);
    }
    let positions: Vec<_> = records
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            if let Record::Data(c) = r {
                Some((i, c.stream.source))
            } else {
                None
            }
        })
        .collect();
    assert!(
        positions.iter().find(|(_, source)| *source == 2).unwrap().0 < positions.last().unwrap().0
    );
    broker.publish(1, vec![3; 1]).unwrap();
    broker.publish(1, vec![4; 1]).unwrap();
    let Record::Status(status) = broker.next_record().unwrap() else {
        panic!("coalescing status");
    };
    assert_eq!(status.coalesced, 1);
    let Record::Data(chunk) = broker.next_record().unwrap() else {
        panic!("latest complete report");
    };
    assert_eq!(chunk.payload.as_slice(), &[4]);
    broker.detach();
    while let Some(record) = broker.next_record() {
        assert!(!matches!(record, Record::Data(_)));
    }
    assert!(broker.register(descriptor(1), None).is_err());
}

#[shoop_test]
fn source_capacity_and_removal_cancel_transfer_and_publish_an_unavailable_epoch() {
    use profiling_protocol::broker::Broker;
    let mut broker = Broker::default();
    for source in 1..=MAX_SOURCES as u32 {
        broker
            .register(
                Descriptor {
                    stream: StreamId { source, ..stream() },
                    name: Name::new("test.source").unwrap(),
                    available: true,
                    min_interval_ms: 1,
                    max_interval_ms: 1,
                    snapshot: true,
                    inspect: false,
                },
                None,
            )
            .unwrap();
    }
    let extra = Descriptor {
        stream: StreamId {
            source: 99,
            ..stream()
        },
        name: Name::new("test.extra").unwrap(),
        available: true,
        min_interval_ms: 1,
        max_interval_ms: 1,
        snapshot: true,
        inspect: false,
    };
    assert_eq!(broker.register(extra, None), Err(Error::Capacity));
    broker.publish(1, vec![1; CHUNK_BYTES * 2]).unwrap();
    broker.remove(1).unwrap();
    assert!(broker.publish(1, vec![2]).is_err());
    let records: Vec<_> = core::iter::from_fn(|| broker.next_record()).collect();
    assert!(records.iter().any(|r| matches!(r, Record::Descriptor(d) if d.stream.source == 1 && d.stream.epoch == stream().epoch + 1 && !d.available)));
    assert!(!records
        .iter()
        .any(|r| matches!(r, Record::Data(c) if c.stream.source == 1)));
}
