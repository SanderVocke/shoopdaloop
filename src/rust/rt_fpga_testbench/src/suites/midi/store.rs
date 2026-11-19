use super::{Memory, command, projection};
use crate::{Result, model::Design, sim::Sim};
use rt_fpga_reference::midi::{ContentStore, Message, MidiState, SnapshotId, StoreLimits, ViewId};
use std::{cell::RefCell, rc::Rc};

fn handle(slot: u32, generation: u32) -> u64 {
    u64::from(slot) | u64::from(generation) << 32
}
fn view(id: ViewId) -> u64 {
    handle(id.slot, id.generation)
}
fn snapshot(id: SnapshotId) -> u64 {
    handle(id.slot, id.generation)
}
fn op(s: &mut Sim<'_>, code: u64, v: u64, state: u64, a: u64, b: u64) -> u64 {
    s.model.set_all(&[
        ("command_op", code),
        ("command_view", v),
        ("command_snapshot", state),
        ("command_arg0", a),
        ("command_arg1", b),
    ]);
    command(s)
}
fn verify(s: &mut Sim<'_>, store: &ContentStore, id: ViewId, h: u64) {
    let desc = store.describe(id).unwrap();
    assert_eq!(op(s, 12, h, 0, 0, 0), 0);
    assert_eq!(s.model.get("response_events"), u64::from(desc.events));
    assert_eq!(s.model.get("response_frame"), desc.frames);
    for i in 0..desc.events {
        assert_eq!(op(s, 10, h, 0, u64::from(i), 0), 0);
        let record = store.read(id, i).unwrap().unwrap();
        assert_eq!(s.model.get("response_frame"), record.frame);
        assert_eq!(s.model.get("response_order"), u64::from(record.order));
        let bytes = record.message.data();
        let expected = bytes
            .iter()
            .enumerate()
            .fold(0, |n, (j, b)| n | u64::from(*b) << (j * 8));
        assert_eq!(
            s.model.get("response_packet") & ((1u64 << (bytes.len() * 8)) - 1),
            expected
        );
        assert_eq!(s.model.get("response_length"), bytes.len() as u64);
    }
    assert_eq!(op(s, 10, h, 0, u64::from(desc.events), 0), 0);
    assert_eq!(s.model.get("response_length"), 0);
}
pub fn store_conformance() -> Result<()> {
    let limits = StoreLimits {
        block_events: 4,
        blocks: 12,
        max_events: 16,
        views: 8,
        snapshots: 8,
        retirements: 8,
    };
    let mut d = Design::new(
        "midi_store",
        vec![crate::root().join("fpga/midi/rtl/midi_store.sv")],
    );
    d.includes.push(crate::root().join("fpga/midi/rtl"));
    let compiled = d
        .parameters(&[
            ("BLOCK_EVENTS", 4),
            ("BLOCKS", 12),
            ("MAX_EVENTS", 16),
            ("VIEWS", 8),
            ("SNAPSHOTS", 8),
            ("RETIREMENTS", 8),
        ])
        .build(&crate::root().join("work/marlin-suite/midi"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
    s.max_ps = 100_000_000_000_000;
    let words = s.model.get("arena_bytes") as usize / 4;
    let memory = Rc::new(RefCell::new(Memory::new(words)));
    s.agent(memory.clone());
    s.agent(crate::protocol::ReadyValid::new(
        "memory_valid",
        "memory_ready",
        &["memory_address", "memory_write", "memory_write_data"],
    ));
    s.model.set("reset", 1);
    s.idle(1);
    s.model.set("reset", 0);
    assert_eq!(op(&mut s, 0, 0, 0, 0, 0), 0);
    let mut store = ContentStore::prepare(limits).unwrap();
    let state = MidiState::default();
    let initial = store.snapshot(&state).unwrap();
    assert_eq!(op(&mut s, 1, 0, 0, 0, 0), 0);
    let initial_handle = s.model.get("response_handle");
    assert_eq!(initial_handle, snapshot(initial));
    let initial_address = s.model.get("response_state") as usize / 4;
    assert_eq!(
        &memory.borrow().words[initial_address..initial_address + 4225],
        projection(&state)
    );
    let source = store.writer(initial).unwrap();
    assert_eq!(op(&mut s, 4, 0, initial_handle, 0, 0), 0);
    let source_handle = s.model.get("response_handle");
    assert_eq!(source_handle, view(source));
    for i in 0..12 {
        let packet = [0xb0, 1, i as u8];
        let frame = i * 2;
        store
            .append(source, frame, Message::managed(&packet).unwrap())
            .unwrap();
        s.model.set_all(&[
            ("command_packet", 0xb0 | 1 << 8 | i << 16),
            ("command_length", 3),
        ]);
        assert_eq!(op(&mut s, 5, source_handle, 0, frame, 0), 0);
    }
    store.finish(source, 30).unwrap();
    assert_eq!(op(&mut s, 6, source_handle, 0, 30, 0), 0);
    verify(&mut s, &store, source, source_handle);
    let range = store.range(source, 5, 22, initial).unwrap();
    assert_eq!(op(&mut s, 7, source_handle, initial_handle, 5, 22), 0);
    let range_handle = s.model.get("response_handle");
    verify(&mut s, &store, range, range_handle);
    store.retain(range).unwrap();
    assert_eq!(op(&mut s, 8, range_handle, 0, 0, 0), 0);
    store.release(source).unwrap();
    assert_eq!(op(&mut s, 9, source_handle, 0, 0, 0), 0);
    assert_eq!(op(&mut s, 12, source_handle, 0, 0, 0), 4);
    for _ in 0..5 {
        let work = store.maintenance(1);
        assert_eq!(op(&mut s, 11, 0, 0, 1, 0), 0);
        assert_eq!(s.model.get("response_work"), u64::from(work));
        verify(&mut s, &store, range, range_handle);
    }
    let next = store.writer(initial).unwrap();
    assert_eq!(op(&mut s, 4, 0, initial_handle, 0, 0), 0);
    let next_handle = s.model.get("response_handle");
    assert_ne!(next_handle, source_handle);
    assert_eq!(op(&mut s, 12, source_handle, 0, 0, 0), 4);
    for i in 0..16 {
        let message = Message::managed(&[0x90, 60, i as u8]).unwrap();
        store.append(next, i, message).unwrap();
        s.model
            .set_all(&[("command_packet", 0x3c90 | i << 16), ("command_length", 3)]);
        assert_eq!(op(&mut s, 5, next_handle, 0, i, 0), 0);
    }
    assert!(
        store
            .append(next, 17, Message::managed(&[0x80, 60, 0]).unwrap())
            .is_err()
    );
    assert_eq!(op(&mut s, 5, next_handle, 0, 17, 0), 3);
    assert!(store.finish(next, 20).is_err());
    assert_eq!(op(&mut s, 6, next_handle, 0, 20, 0), 4);
    verify(&mut s, &store, next, next_handle);
    for (id, h) in [
        (range, range_handle),
        (range, range_handle),
        (next, next_handle),
    ] {
        store.release(id).unwrap();
        assert_eq!(op(&mut s, 9, h, 0, 0, 0), 0);
    }
    let work = store.maintenance(100);
    assert_eq!(op(&mut s, 11, 0, 0, 100, 0), 0);
    assert_eq!(s.model.get("response_work"), u64::from(work));
    store.release_snapshot(initial).unwrap();
    assert_eq!(op(&mut s, 3, 0, initial_handle, 0, 0), 0);
    assert_eq!(op(&mut s, 15, 0, initial_handle, 0, 0), 4);
    let replacement = store.snapshot(&state).unwrap();
    assert_eq!(op(&mut s, 1, 0, 0, 0, 0), 0);
    assert_eq!(s.model.get("response_handle"), snapshot(replacement));
    assert_ne!(snapshot(replacement), initial_handle);
    assert_eq!(op(&mut s, 2, 0, initial_handle, 0, 0), 4);
    let writer = store.writer(replacement).unwrap();
    assert_eq!(op(&mut s, 4, 0, snapshot(replacement), 0, 0), 0);
    let writer_handle = s.model.get("response_handle");
    for i in 0..10 {
        store
            .append(
                writer,
                i * 2,
                Message::managed(&[0x91, 61, i as u8]).unwrap(),
            )
            .unwrap();
        s.model
            .set_all(&[("command_packet", 0x3d91 | i << 16), ("command_length", 3)]);
        assert_eq!(op(&mut s, 5, writer_handle, 0, i * 2, 0), 0);
    }
    store.advance(writer, 30).unwrap();
    assert_eq!(op(&mut s, 13, writer_handle, 0, 30, 0), 0);
    let grabbed = store.range(writer, 3, 17, replacement).unwrap();
    assert_eq!(
        op(&mut s, 7, writer_handle, snapshot(replacement), 3, 17),
        0
    );
    let grabbed_handle = s.model.get("response_handle");
    let discarded = store.discard_before(writer, 8).unwrap();
    assert_eq!(discarded, 4);
    assert_eq!(op(&mut s, 14, writer_handle, 0, u64::from(discarded), 0), 0);
    store
        .append(writer, 31, Message::managed(&[0x81, 61, 0]).unwrap())
        .unwrap();
    s.model.set("command_packet", 0x3d81);
    assert_eq!(op(&mut s, 5, writer_handle, 0, 31, 0), 0);
    store.advance(writer, 32).unwrap();
    assert_eq!(op(&mut s, 13, writer_handle, 0, 32, 0), 0);
    verify(&mut s, &store, writer, writer_handle);
    verify(&mut s, &store, grabbed, grabbed_handle);
    let nested = store.range(grabbed, 2, 8, replacement).unwrap();
    assert_eq!(
        op(&mut s, 7, grabbed_handle, snapshot(replacement), 2, 8),
        0
    );
    let nested_handle = s.model.get("response_handle");
    verify(&mut s, &store, nested, nested_handle);
    let empty = store.range(grabbed, 2, 2, replacement).unwrap();
    assert_eq!(
        op(&mut s, 7, grabbed_handle, snapshot(replacement), 2, 2),
        0
    );
    let empty_handle = s.model.get("response_handle");
    verify(&mut s, &store, empty, empty_handle);
    store.release(writer).unwrap();
    assert_eq!(op(&mut s, 9, writer_handle, 0, 0, 0), 0);
    store.maintenance(100);
    assert_eq!(op(&mut s, 11, 0, 0, 100, 0), 0);
    verify(&mut s, &store, grabbed, grabbed_handle);
    verify(&mut s, &store, nested, nested_handle);
    Ok(())
}
