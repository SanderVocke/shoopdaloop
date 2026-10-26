#[path = "../../rt_runtime/tests/support/mod.rs"]
mod support;
use shoop_loop_engine::{
    topology::staging::{StageId, Stager, Status, RECORD_BYTES},
    *,
};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

type Bytes = [u8; RECORD_BYTES];
fn id(slot: u32) -> LoopId {
    LoopId::from_parts(slot, 1).unwrap()
}
fn limits() -> Limits {
    Limits {
        loop_slots: 4,
        relations: 8,
        program_entries: 8,
        program_segments: 16,
        pending_requests: 4,
        journal_events: 0,
    }
}
fn put32(bytes: &mut Bytes, at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(bytes: &mut Bytes, at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn identity(bytes: &mut Bytes, at: usize, slot: u32) {
    put32(bytes, at, slot);
    put32(bytes, at + 4, 1);
}
fn header(limits: Limits, counts: [u32; 7], relations: u32) -> Bytes {
    let mut bytes = [0; RECORD_BYTES];
    bytes[..4].copy_from_slice(b"LTP1");
    put32(&mut bytes, 4, counts.iter().sum());
    for (i, value) in [
        limits.loop_slots,
        limits.relations,
        limits.program_entries,
        limits.program_segments,
        limits.pending_requests,
        limits.journal_events,
    ]
    .into_iter()
    .enumerate()
    {
        put32(&mut bytes, 8 + i * 4, value as u32);
    }
    for (i, value) in counts.into_iter().enumerate() {
        put32(&mut bytes, 32 + i * 4, value);
    }
    put32(&mut bytes, 60, relations);
    bytes
}
fn node(slot: u32, kind: LoopKind, length: u64, sync: Option<u32>) -> LoopSpec {
    LoopSpec {
        id: id(slot),
        kind,
        length,
        sync_source: sync.map(id),
    }
}
fn fixture() -> PreparedTopology {
    compile(
        limits(),
        &TopologyDescription {
            loops: vec![
                node(0, LoopKind::Primitive, 4, None),
                node(1, LoopKind::Primitive, 2, None),
                node(2, LoopKind::Composite, 0, Some(0)),
            ],
            triggers: vec![Trigger {
                source: id(0),
                target: id(1),
                mode: Mode::Playing,
            }],
            programs: vec![ProgramDescription {
                source: id(2),
                nominal_unit_frames: 2,
                sections: vec![
                    Section {
                        entries: vec![Entry {
                            target: id(1),
                            delay: 1,
                            duration: Some(2),
                            repetitions: 1,
                            mode: Some(Mode::Recording),
                        }],
                    },
                    Section {
                        entries: vec![Entry {
                            target: id(1),
                            delay: 0,
                            duration: Some(1),
                            repetitions: 1,
                            mode: Some(Mode::Stopped),
                        }],
                    },
                ],
            }],
        },
    )
    .unwrap()
}
fn golden() -> (Bytes, Vec<Bytes>) {
    let mut rows = vec![[0; RECORD_BYTES]; 14];
    for (slot, length) in [(0, 4), (1, 2), (2, 8)] {
        identity(&mut rows[slot], 0, slot as u32);
        put64(&mut rows[slot], 16, length);
    }
    rows[2][8] = 1;
    rows[2][9] = 1;
    identity(&mut rows[2], 24, 0);
    identity(&mut rows[3], 0, 0);
    identity(&mut rows[3], 8, 1);
    rows[3][16] = 1;
    identity(&mut rows[4], 0, 2);
    rows[4][8] = 1;
    put64(&mut rows[4], 16, 2);
    put64(&mut rows[4], 24, 4);
    put32(&mut rows[4], 36, 1);
    put32(&mut rows[4], 44, 2);
    identity(&mut rows[5], 0, 1);
    put32(&mut rows[5], 12, 2);
    for (index, start, end, ordinal, mode) in [(6, 1, 3, 0, 2), (7, 3, 4, 1, 0)] {
        put64(&mut rows[index], 0, start);
        put64(&mut rows[index], 8, end);
        put64(&mut rows[index], 16, start);
        put32(&mut rows[index], 24, ordinal);
        rows[index][28] = mode;
    }
    for index in 0..3 {
        put32(&mut rows[8 + index], 0, index as u32);
        put32(&mut rows[8 + index], 4, 1);
    }
    for (index, slot) in [0, 2, 1].into_iter().enumerate() {
        identity(&mut rows[11 + index], 0, slot);
    }
    (header(limits(), [3, 1, 1, 1, 2, 3, 3], 3), rows)
}
fn encode(topology: &PreparedTopology) -> (Bytes, Vec<Bytes>) {
    let mut header = [0; RECORD_BYTES];
    assert_no_heap(|| topology.encode_header(&mut header));
    let mut rows = vec![[0; RECORD_BYTES]; topology.encoded_records()];
    for (index, row) in rows.iter_mut().enumerate() {
        assert_no_heap(|| topology.encode_record(index, row)).unwrap();
    }
    (header, rows)
}
fn append(stager: &mut Stager, token: StageId, rows: &[Bytes], chunk: usize) {
    let mut offset = 0;
    let mut packet = [0; RECORD_BYTES * 16];
    while offset < rows.len() {
        let count = chunk.min(rows.len() - offset);
        assert!(count <= 16);
        for (index, row) in rows[offset..offset + count].iter().enumerate() {
            packet[index * RECORD_BYTES..(index + 1) * RECORD_BYTES].copy_from_slice(row);
        }
        assert_no_heap(|| stager.append(token, offset, &packet[..count * RECORD_BYTES], count))
            .unwrap();
        offset += count;
    }
}
fn settle(stager: &mut Stager, token: StageId, budget: usize) -> Result<(), Error> {
    let mut work = 0;
    loop {
        let progress = assert_no_heap(|| stager.validate(token, budget))?;
        assert!(progress.used <= budget);
        work += progress.used;
        // These fixtures have <= 8 edges and <= 4 nodes: this bound exceeds
        // their row scans, quadratic edge deduplication and component scans.
        assert!(work < 2000);
        if progress.ready {
            return Ok(());
        }
        assert!(progress.used > 0);
    }
}

#[shoop_test]
fn canonical_bytes_roundtrip_across_chunk_and_validation_budgets_without_heap_work() {
    let original = fixture();
    let (bytes, rows) = golden();
    assert_eq!(encode(&original), (bytes, rows.clone()));
    let mut stager = Stager::prepare(limits()).unwrap();
    let mut previous = None;
    for (chunk, budget) in [(1, 1), (2, 3), (7, usize::MAX), (14, 1)] {
        let token = assert_no_heap(|| stager.begin(&bytes)).unwrap();
        if let Some(old) = previous {
            assert_eq!(assert_no_heap(|| stager.cancel(old)), Err(Error::InvalidId));
        }
        assert_eq!(
            StageId::from_parts(token.parts()[0], token.parts()[1]),
            Ok(token)
        );
        assert_no_heap(|| {
            assert_eq!(stager.view(token), Err(Error::InvalidState));
            assert_eq!(stager.take(token), Err(Error::InvalidState));
        });
        append(&mut stager, token, &rows, chunk);
        assert_no_heap(|| {
            let progress = stager.validate(token, 0).unwrap();
            assert_eq!(progress.used, 0);
            assert_eq!(progress.total_steps, 0);
            assert!(!progress.ready);
            assert_eq!(stager.view(token), Err(Error::InvalidState));
        });
        settle(&mut stager, token, budget).unwrap();
        assert_no_heap(|| {
            assert_eq!(stager.view(token).unwrap(), &original);
            assert_eq!(stager.validate(token, 0).unwrap().used, 0);
        });
        let ready = assert_no_heap(|| stager.take(token)).unwrap();
        assert_eq!(
            ready.storage_bytes(),
            limits().topology_storage_bytes().unwrap()
        );
        assert_eq!(encode(&ready), (bytes, rows.clone()));
        assert_eq!(
            assert_no_heap(|| stager.begin(&bytes)),
            Err(Error::InvalidState)
        );
        assert_eq!(
            assert_no_heap(|| stager.cancel(token)),
            Err(Error::InvalidState)
        );
        assert_no_heap(|| stager.recycle(ready)).unwrap();
        assert_eq!(stager.status(), Status::Idle);
        previous = Some(token);
    }
    let mut untouched = [91; RECORD_BYTES];
    assert_eq!(
        assert_no_heap(|| original.encode_record(usize::MAX, &mut untouched)),
        Err(Error::InvalidId)
    );
    assert_eq!(untouched, [91; RECORD_BYTES]);
    assert_eq!(StageId::from_parts(0, 1), Err(Error::InvalidId));
    assert_eq!(StageId::from_parts(1, 0), Err(Error::InvalidId));
}

#[shoop_test]
fn header_chunk_and_scope_refusals_preserve_received_prefix_and_owning_recycling() {
    let (bytes, rows) = golden();
    let mut stager = Stager::prepare(limits()).unwrap();
    let mut other = Stager::prepare(limits()).unwrap();
    for (offset, value, expected) in [
        (0, 0, Error::InvalidProgram),
        (24, 5, Error::InvalidConfig),
        (32, 5, Error::Capacity),
        (4, 99, Error::InvalidProgram),
        (52, 4, Error::InvalidProgram),
        (56, 2, Error::InvalidProgram),
        (60, 9, Error::Capacity),
    ] {
        let mut bad = bytes;
        put32(&mut bad, offset, value);
        assert_eq!(assert_no_heap(|| stager.begin(&bad)), Err(expected));
        assert_eq!(stager.status(), Status::Idle);
    }
    let token = assert_no_heap(|| stager.begin(&bytes)).unwrap();
    assert_eq!(token.parts()[1], 1);
    let foreign = assert_no_heap(|| other.begin(&bytes)).unwrap();
    assert_eq!(
        assert_no_heap(|| stager.append(foreign, 0, &rows[0], 1)),
        Err(Error::InvalidId)
    );
    assert_eq!(
        assert_no_heap(|| stager.append(token, 0, &rows[0], 0)),
        Err(Error::Capacity)
    );
    assert_eq!(
        assert_no_heap(|| stager.append(token, 1, &rows[0], 1)),
        Err(Error::InvalidState)
    );
    assert_eq!(
        assert_no_heap(|| stager.append(token, 0, &rows[0][..63], 1)),
        Err(Error::InvalidProgram)
    );
    let mut packet = [0; RECORD_BYTES * 2];
    packet[..RECORD_BYTES].copy_from_slice(&rows[0]);
    packet[RECORD_BYTES..].copy_from_slice(&rows[1]);
    packet[RECORD_BYTES + 63] = 1;
    assert_eq!(
        assert_no_heap(|| stager.append(token, 0, &packet, 2)),
        Err(Error::InvalidProgram)
    );
    assert_eq!(
        stager.status(),
        Status::Receiving {
            received: 0,
            total: 14
        }
    );
    assert_no_heap(|| stager.append(token, 0, &[], 0)).unwrap();
    assert_no_heap(|| stager.append(token, 0, &rows[0], 1)).unwrap();
    assert_eq!(
        assert_no_heap(|| stager.append(token, 0, &rows[0], 1)),
        Err(Error::InvalidState)
    );
    for row in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13] {
        assert_no_heap(|| stager.append(token, row, &rows[row], 1)).unwrap();
    }
    settle(&mut stager, token, 1).unwrap();
    let ready = assert_no_heap(|| stager.take(token)).unwrap();
    let undersized = fixture();
    let (error, undersized) = assert_no_heap(|| stager.recycle(undersized)).unwrap_err();
    assert_eq!(error, Error::Capacity);
    assert_eq!(encode(&undersized), (bytes, rows.clone()));
    drop(undersized);
    let wrong_limits = compile(Limits::default(), &TopologyDescription::default()).unwrap();
    let (error, wrong_limits) = assert_no_heap(|| stager.recycle(wrong_limits)).unwrap_err();
    assert_eq!(error, Error::InvalidConfig);
    drop(wrong_limits);
    let (error, ready) = assert_no_heap(|| other.recycle(ready)).unwrap_err();
    assert_eq!(error, Error::InvalidState);
    append(&mut other, foreign, &rows, 7);
    settle(&mut other, foreign, 7).unwrap();
    let other_ready = assert_no_heap(|| other.take(foreign)).unwrap();
    assert_no_heap(|| stager.recycle(other_ready)).unwrap();
    assert_no_heap(|| other.recycle(ready)).unwrap();
    for receiver in [&mut stager, &mut other] {
        let token = assert_no_heap(|| receiver.begin(&bytes)).unwrap();
        assert_no_heap(|| receiver.append(token, 0, &rows[0], 1)).unwrap();
        assert_no_heap(|| receiver.cancel(token)).unwrap();
        assert_eq!(receiver.status(), Status::Idle);
    }
    assert!(
        assert_no_heap(|| Stager::required_storage_bytes(limits())).unwrap()
            > limits().topology_storage_bytes().unwrap()
    );
    assert_eq!(
        assert_no_heap(|| Stager::required_storage_bytes(Limits {
            relations: usize::MAX,
            ..limits()
        })),
        Err(Error::Capacity)
    );
}

#[shoop_test]
fn malformed_semantics_never_publish_and_failed_storage_can_be_reused() {
    let (bytes, rows) = golden();
    let mut stager = Stager::prepare(limits()).unwrap();
    let changes: [(usize, usize, u64, usize, Error); 21] = [
        (1, 0, 0, 4, Error::DuplicateId),
        (2, 24, 3, 4, Error::InvalidId),
        (4, 0, 1, 4, Error::InvalidProgram),
        (4, 16, 0, 8, Error::InvalidProgram),
        (4, 16, u64::MAX, 8, Error::Exhausted),
        (2, 16, 9, 8, Error::InvalidProgram),
        (4, 32, 1, 4, Error::InvalidProgram),
        (4, 36, 2, 4, Error::InvalidProgram),
        (4, 40, 1, 4, Error::InvalidProgram),
        (5, 12, 0, 4, Error::InvalidProgram),
        (6, 8, 1, 8, Error::InvalidProgram),
        (6, 16, 0, 8, Error::InvalidProgram),
        (6, 8, 5, 8, Error::InvalidProgram),
        (7, 0, 2, 8, Error::InvalidProgram),
        (6, 24, u32::MAX as u64, 4, Error::InvalidProgram),
        (7, 24, 0, 4, Error::InvalidProgram),
        (8, 0, 1, 4, Error::InvalidProgram),
        (8, 4, 0, 4, Error::InvalidProgram),
        (12, 0, 0, 4, Error::InvalidProgram),
        (12, 4, 2, 4, Error::InvalidId),
        (0, 0, u32::MAX as u64, 4, Error::Capacity),
    ];
    for (row, offset, value, width, expected) in changes {
        let mut bad = rows.clone();
        if width == 8 {
            put64(&mut bad[row], offset, value);
        } else {
            put32(&mut bad[row], offset, value as u32);
        }
        let token = assert_no_heap(|| stager.begin(&bytes)).unwrap();
        append(&mut stager, token, &bad, 7);
        assert_eq!(
            settle(&mut stager, token, 1),
            Err(expected),
            "row {row} offset {offset}"
        );
        assert_no_heap(|| {
            assert_eq!(stager.status(), Status::Failed(expected));
            assert_eq!(stager.view(token), Err(Error::InvalidState));
            assert_eq!(stager.take(token), Err(Error::InvalidState));
            assert_eq!(stager.validate(token, usize::MAX), Err(expected));
            stager.cancel(token).unwrap();
        });
    }
    for (mut bad, expected) in [
        (rows.clone(), Error::RecursiveProgram),
        (rows.clone(), Error::InstantaneousDependencyCycle),
    ] {
        if expected == Error::RecursiveProgram {
            put32(&mut bad[5], 0, 2);
            bad[6][28] = 1;
        } else {
            put32(&mut bad[3], 8, 0);
        }
        let token = assert_no_heap(|| stager.begin(&bytes)).unwrap();
        append(&mut stager, token, &bad, 14);
        assert_eq!(settle(&mut stager, token, 3), Err(expected));
        assert_no_heap(|| stager.cancel(token)).unwrap();
    }
    // Reserved bytes, invalid enum tags and zero generations are transactional syntax failures.
    for (row, offset, value) in [
        (0, 63, 1),
        (2, 8, 9),
        (2, 9, 2),
        (3, 16, 7),
        (4, 8, 4),
        (6, 28, 3),
        (0, 4, 0),
    ] {
        let mut bad = rows.clone();
        bad[row][offset] = value;
        let token = assert_no_heap(|| stager.begin(&bytes)).unwrap();
        for (index, bytes) in bad.iter().enumerate() {
            let result = assert_no_heap(|| stager.append(token, index, bytes, 1));
            if index == row {
                assert!(result.is_err());
                assert_eq!(
                    stager.status(),
                    Status::Receiving {
                        received: row,
                        total: 14
                    }
                );
                break;
            } else {
                result.unwrap();
            }
        }
        assert_no_heap(|| stager.cancel(token)).unwrap();
    }
    let token = assert_no_heap(|| stager.begin(&bytes)).unwrap();
    append(&mut stager, token, &rows, 2);
    settle(&mut stager, token, usize::MAX).unwrap();
    assert_eq!(encode(stager.view(token).unwrap()), (bytes, rows));
    assert_no_heap(|| stager.cancel(token)).unwrap();
}

fn graph_reference(sync: [Option<usize>; 3], bits: usize) -> (bool, Vec<Vec<usize>>) {
    let actions = [(0, 1), (1, 2), (2, 0)];
    let mut reach = [[false; 3]; 3];
    let mut edges = Vec::new();
    for node in 0..3 {
        reach[node][node] = true;
        if let Some(source) = sync[node] {
            reach[source][node] = true;
            edges.push((source, node));
        }
    }
    for (index, (source, target)) in actions.into_iter().enumerate() {
        if bits & (1 << index) != 0 {
            reach[source][target] = true;
            edges.push((source, target));
        }
    }
    for middle in 0..3 {
        for source in 0..3 {
            for target in 0..3 {
                reach[source][target] |= reach[source][middle] && reach[middle][target];
            }
        }
    }
    let valid = actions
        .into_iter()
        .enumerate()
        .all(|(index, (source, target))| bits & (1 << index) == 0 || !reach[target][source]);
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut assignment = [usize::MAX; 3];
    for node in 0..3 {
        if assignment[node] != usize::MAX {
            continue;
        }
        let members = (0..3)
            .filter(|other| reach[node][*other] && reach[*other][node])
            .collect::<Vec<_>>();
        for member in &members {
            assignment[*member] = groups.len();
        }
        groups.push(members);
    }
    let mut ordered = Vec::new();
    let mut emitted = vec![false; groups.len()];
    while ordered.len() != groups.len() {
        let choice = (0..groups.len())
            .filter(|group| {
                !emitted[*group]
                    && !edges.iter().any(|(source, target)| {
                        assignment[*target] == *group
                            && assignment[*source] != *group
                            && !emitted[assignment[*source]]
                    })
            })
            .min_by_key(|group| groups[*group][0])
            .unwrap();
        emitted[choice] = true;
        ordered.push(groups[choice].clone());
    }
    (valid, ordered)
}
fn graph_bytes(
    limits: Limits,
    sync: [Option<usize>; 3],
    bits: usize,
    groups: &[Vec<usize>],
) -> (Bytes, Vec<Bytes>, TopologyDescription) {
    let mut rows = Vec::new();
    let mut description = TopologyDescription::default();
    let mut relations = 0;
    for (slot, source) in sync.into_iter().enumerate() {
        description.loops.push(node(
            slot as u32,
            LoopKind::Primitive,
            1,
            source.map(|v| v as u32),
        ));
        let mut row = [0; RECORD_BYTES];
        identity(&mut row, 0, slot as u32);
        put64(&mut row, 16, 1);
        if let Some(source) = source {
            row[9] = 1;
            identity(&mut row, 24, source as u32);
            relations += 1;
        }
        rows.push(row);
    }
    for (index, (source, target)) in [(0, 1), (1, 2), (2, 0)].into_iter().enumerate() {
        if bits & (1 << index) == 0 {
            continue;
        }
        description.triggers.push(Trigger {
            source: id(source),
            target: id(target),
            mode: Mode::Playing,
        });
        let mut row = [0; RECORD_BYTES];
        identity(&mut row, 0, source);
        identity(&mut row, 8, target);
        row[16] = 1;
        rows.push(row);
        relations += 1;
    }
    let mut start = 0;
    for group in groups {
        let mut row = [0; RECORD_BYTES];
        put32(&mut row, 0, start);
        put32(&mut row, 4, group.len() as u32);
        start += group.len() as u32;
        rows.push(row);
    }
    for member in groups.iter().flatten() {
        let mut row = [0; RECORD_BYTES];
        identity(&mut row, 0, *member as u32);
        rows.push(row);
    }
    (
        header(
            limits,
            [
                3,
                description.triggers.len() as u32,
                0,
                0,
                0,
                groups.len() as u32,
                3,
            ],
            relations,
        ),
        rows,
        description,
    )
}
#[shoop_test]
fn all_small_sync_action_graphs_match_independent_reachability_and_canonical_sccs() {
    let limits = Limits {
        loop_slots: 3,
        relations: 6,
        program_entries: 1,
        program_segments: 1,
        pending_requests: 1,
        journal_events: 0,
    };
    let mut stager = Stager::prepare(limits).unwrap();
    let mut accepted = 0;
    let mut refused = 0;
    for choices in 0..64 {
        let sync = std::array::from_fn(|node| {
            let choice = (choices >> (node * 2)) & 3;
            (choice != 3).then_some(choice)
        });
        for bits in 0..8 {
            let (valid, groups) = graph_reference(sync, bits);
            let (bytes, rows, description) = graph_bytes(limits, sync, bits, &groups);
            let token = assert_no_heap(|| stager.begin(&bytes)).unwrap();
            append(&mut stager, token, &rows, 1 + (choices + bits) % 4);
            let result = settle(&mut stager, token, [1, 7, usize::MAX][(choices + bits) % 3]);
            if valid {
                result.unwrap();
                let compiled = compile(limits, &description).unwrap();
                assert_no_heap(|| {
                    let view = stager.view(token).unwrap();
                    assert_eq!(view, &compiled);
                    for (actual, expected) in view.components().into_iter().zip(&groups) {
                        assert!(actual
                            .iter()
                            .map(|id| id.slot() as usize)
                            .eq(expected.iter().copied()));
                    }
                });
                let ready = assert_no_heap(|| stager.take(token)).unwrap();
                assert_no_heap(|| stager.recycle(ready)).unwrap();
                accepted += 1;
            } else {
                assert_eq!(result, Err(Error::InstantaneousDependencyCycle));
                assert_eq!(
                    compile(limits, &description).unwrap_err(),
                    Error::InstantaneousDependencyCycle
                );
                assert_no_heap(|| stager.cancel(token)).unwrap();
                refused += 1;
            }
        }
    }
    assert_eq!(accepted + refused, 512);
    assert!(accepted > 50 && refused > 50);
    // A falsely merged cycle plus a downstream tree is not strongly connected.
    let (bytes, rows, _) = graph_bytes(limits, [Some(1), Some(0), Some(1)], 0, &[vec![0, 1, 2]]);
    let token = assert_no_heap(|| stager.begin(&bytes)).unwrap();
    append(&mut stager, token, &rows, 3);
    assert_eq!(settle(&mut stager, token, 1), Err(Error::InvalidProgram));
    assert_no_heap(|| stager.cancel(token)).unwrap();
    // Structurally valid SCCs must also use canonical ready-group tie order.
    let (bytes, rows, _) = graph_bytes(limits, [None; 3], 0, &[vec![1], vec![0], vec![2]]);
    let token = assert_no_heap(|| stager.begin(&bytes)).unwrap();
    append(&mut stager, token, &rows, 3);
    assert_eq!(settle(&mut stager, token, 1), Err(Error::InvalidProgram));
    assert_no_heap(|| stager.cancel(token)).unwrap();
}

#[shoop_test]
fn decoded_owners_replace_and_recycle_through_the_real_resolver_without_callback_drops() {
    let (bytes, rows) = golden();
    let mut a = Stager::prepare(limits()).unwrap();
    let mut b = Stager::prepare(limits()).unwrap();
    let token = assert_no_heap(|| a.begin(&bytes)).unwrap();
    append(&mut a, token, &rows, 14);
    settle(&mut a, token, usize::MAX).unwrap();
    let initial = assert_no_heap(|| a.take(token)).unwrap();
    let mut resolver = Resolver::prepare(initial).unwrap();
    for sequence in 0..128 {
        let receiver = if sequence % 2 == 0 { &mut b } else { &mut a };
        let token = assert_no_heap(|| receiver.begin(&bytes)).unwrap();
        append(receiver, token, &rows, 7);
        settle(receiver, token, 7).unwrap();
        let candidate = assert_no_heap(|| receiver.take(token)).unwrap();
        let (error, candidate) =
            assert_no_heap(|| resolver.replace_topology(Epoch(u64::MAX), candidate)).unwrap_err();
        assert_eq!(error, Error::InvalidState);
        let retired =
            assert_no_heap(|| resolver.replace_topology(Epoch(sequence), candidate)).unwrap();
        let recycler = if sequence % 2 == 0 { &mut a } else { &mut b };
        assert_no_heap(|| recycler.recycle(retired)).unwrap();
        assert_eq!(resolver.epoch(), Epoch(sequence + 1));
        assert_eq!(resolver.frame(), Frame(0));
        assert_no_heap(|| assert_eq!(resolver.topology().program(id(2)).unwrap().iterations, 4));
    }
    // The active owner stays with the resolver; the other bank is empty and
    // waiting for that owner. Their eventual destruction is deliberately off-render.
    assert_eq!(a.status(), Status::Taken);
    assert_eq!(b.status(), Status::Idle);
}

#[shoop_test]
fn partial_and_failed_candidate_validation_does_not_change_active_clock_progression() {
    let original = fixture();
    let (bytes, mut rows) = golden();
    put32(&mut rows[12], 4, 2);
    let mut resolver = Resolver::prepare(original).unwrap();
    assert_no_heap(|| {
        resolver.submit(Request {
            target: id(0),
            sequence: Sequence(1),
            at: Frame(0),
            timing: Timing::At,
            transition: Transition::Play {
                position: 0,
                retrigger: false,
            },
        })
    })
    .unwrap();
    let mut stager = Stager::prepare(limits()).unwrap();
    let token = assert_no_heap(|| stager.begin(&bytes)).unwrap();
    let mut failed = false;
    for frame in 0..128 {
        assert_no_heap(|| {
            if frame < rows.len() {
                stager.append(token, frame, &rows[frame], 1).unwrap();
            } else if stager.status() == Status::Validating {
                match stager.validate(token, 1) {
                    Ok(progress) => assert!(!progress.ready),
                    Err(error) => {
                        assert_eq!(error, Error::InvalidId);
                        failed = true;
                    }
                }
            }
            assert_eq!(stager.view(token), Err(Error::InvalidState));
            let interval = resolver.preview(Frame(frame as u64 + 1), &[]).unwrap();
            assert_eq!(interval.end, Frame(frame as u64 + 1));
            resolver.commit().unwrap();
            for (target, length) in [(0, 4), (1, 2)] {
                let state = resolver.state(id(target)).unwrap();
                assert_eq!(state.mode, Mode::Playing);
                assert_eq!(state.position, frame as u64 % length);
                assert_eq!(state.cycles, frame as u64 / length);
                assert_eq!(
                    state.last_pulse,
                    Some(Frame(frame as u64 - frame as u64 % length))
                );
            }
            assert_eq!(resolver.epoch(), Epoch(0));
            assert_eq!(resolver.retained_requests(), 1);
            resolver.finish(interval.end).unwrap();
        });
    }
    assert!(failed);
    assert_no_heap(|| stager.cancel(token)).unwrap();
    assert_eq!(stager.status(), Status::Idle);
}

#[shoop_test]
fn nested_regular_and_explicit_images_preserve_raw_entry_modes_and_sparse_extremes() {
    let limits = Limits {
        loop_slots: 5,
        relations: 16,
        program_entries: 32,
        program_segments: 64,
        pending_requests: 4,
        journal_events: 0,
    };
    let mut stager = Stager::prepare(limits).unwrap();
    for case in 0..128 {
        let repetitions = 1 + case % 3;
        let child_unit = 1 + case % 2;
        let child_entries = vec![
            Entry {
                target: id(1),
                delay: 0,
                duration: Some(6),
                repetitions: repetitions as u32,
                mode: Some(Mode::Playing),
            },
            Entry {
                target: id(1),
                delay: 1,
                duration: Some(2),
                repetitions: 1,
                mode: Some(Mode::Recording),
            },
            Entry {
                target: id(2),
                delay: case % 3,
                duration: Some(4),
                repetitions: 1,
                mode: Some(if case % 2 == 0 {
                    Mode::Stopped
                } else {
                    Mode::Playing
                }),
            },
        ];
        let child_iterations = 6 * repetitions;
        let mut input = TopologyDescription {
            loops: vec![
                node(0, LoopKind::Primitive, 3, None),
                node(1, LoopKind::Primitive, 5, None),
                node(2, LoopKind::Primitive, 7, None),
                node(3, LoopKind::Composite, 0, Some(0)),
                node(4, LoopKind::Composite, 0, Some(0)),
            ],
            triggers: vec![],
            programs: vec![
                ProgramDescription {
                    source: id(3),
                    nominal_unit_frames: child_unit,
                    sections: vec![Section {
                        entries: child_entries.clone(),
                    }],
                },
                ProgramDescription {
                    source: id(4),
                    nominal_unit_frames: 1,
                    sections: vec![Section {
                        entries: vec![
                            Entry {
                                target: id(3),
                                delay: case % 4,
                                duration: None,
                                repetitions: 2,
                                mode: None,
                            },
                            Entry {
                                target: id(2),
                                delay: 0,
                                duration: Some(2),
                                repetitions: 1,
                                mode: None,
                            },
                        ],
                    }],
                },
            ],
        };
        if case % 2 == 0 {
            input.loops.reverse();
            input.programs.reverse();
        }
        let compiled = compile(limits, &input).unwrap();
        let (bytes, rows) = encode(&compiled);
        let token = assert_no_heap(|| stager.begin(&bytes)).unwrap();
        append(&mut stager, token, &rows, 1 + case as usize % 16);
        settle(&mut stager, token, [1, 7, usize::MAX][case as usize % 3]).unwrap();
        assert_no_heap(|| {
            let view = stager.view(token).unwrap();
            assert_eq!(view, &compiled);
            let child = view.program(id(3)).unwrap();
            assert_eq!(child.iterations, child_iterations);
            assert_eq!(child.nominal_frames(), Ok(child_iterations * child_unit));
            assert_eq!(
                view.program(id(4)).unwrap().iterations,
                case % 4 + 2 * child_iterations * child_unit
            );
            for target in [id(1), id(2)] {
                for iteration in 0..=child_iterations {
                    let mut expected = None;
                    for entry in &child_entries {
                        if entry.target != target {
                            continue;
                        }
                        for repetition in 0..entry.repetitions {
                            let start = entry.delay + repetition as u64 * entry.duration.unwrap();
                            let end = start + entry.duration.unwrap();
                            if start <= iteration && iteration < end {
                                expected = entry.mode;
                            }
                        }
                    }
                    assert_eq!(
                        child
                            .target(target)
                            .unwrap()
                            .at(iteration)
                            .map(|segment| segment.mode),
                        expected
                    );
                }
            }
        });
        let ready = assert_no_heap(|| stager.take(token)).unwrap();
        assert_no_heap(|| stager.recycle(ready)).unwrap();
    }
    let input = TopologyDescription {
        loops: vec![
            node(0, LoopKind::Primitive, 1, None),
            node(1, LoopKind::Composite, 0, None),
        ],
        triggers: vec![],
        programs: vec![ProgramDescription {
            source: id(1),
            nominal_unit_frames: 1,
            sections: vec![Section {
                entries: vec![Entry {
                    target: id(0),
                    delay: u64::MAX - 1,
                    duration: Some(1),
                    repetitions: 1,
                    mode: None,
                }],
            }],
        }],
    };
    let compiled = compile(limits, &input).unwrap();
    let (bytes, rows) = encode(&compiled);
    let token = assert_no_heap(|| stager.begin(&bytes)).unwrap();
    append(&mut stager, token, &rows, 2);
    settle(&mut stager, token, 1).unwrap();
    assert_no_heap(|| {
        let view = stager.view(token).unwrap();
        assert_eq!(view, &compiled);
        let target = view.program(id(1)).unwrap().target(id(0)).unwrap();
        assert_eq!(target.at(u64::MAX - 1).unwrap().end, u64::MAX);
        assert_eq!(target.at(u64::MAX), None);
        stager.cancel(token).unwrap();
    });
}
