//! MIDI storage boundary controls independent of its DDR layout and allocator.
use super::Memory;
use crate::{Result, model::Design, sim::Sim};
use rt_fpga_reference::midi::{ContentStore, Error, Message, StoreLimits};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

fn command(
    s: &mut Sim<'_>,
    memory: &Rc<RefCell<Memory>>,
    rows: &mut Vec<Value>,
    op: u64,
    handle: u64,
    arg: u64,
    packet: u64,
    error: u64,
) -> Value {
    let start = s.cycles;
    let transfers = memory.borrow().transfers;
    s.model.set_all(&[
        ("command_valid", 1),
        ("command_op", op),
        ("command_view", handle),
        ("command_arg0", arg),
        ("command_arg1", 0),
        ("command_packet", packet),
        ("command_length", 3),
        ("response_ready", 0),
    ]);
    let mut accepted = false;
    for _ in 0..100000 {
        if s.tick(&["command_ready"]).0.get("command_ready") != 0 {
            accepted = true;
            break;
        }
    }
    assert!(accepted, "MIDI store admission");
    s.model.set_all(&[
        ("command_valid", 0),
        ("command_op", 15),
        ("command_view", u64::MAX),
        ("command_arg0", u64::MAX),
        ("command_arg1", u64::MAX),
        ("command_packet", 0),
    ]);
    s.until(100000, "MIDI store result", |m| {
        m.get("response_valid") != 0
    });
    assert_eq!(
        s.model.get("response_error"),
        error,
        "op{op} handle{handle} arg{arg}"
    );
    let result = json!({"handle":s.model.get("response_handle"),"frame":s.model.get("response_frame"),"order":s.model.get("response_order"),
        "packet":s.model.get("response_packet"),"length":s.model.get("response_length"),
        "work":s.model.get("response_work")});
    s.idle(3);
    s.model.set("response_ready", 1);
    s.idle(1);
    s.model.set("response_ready", 0);
    rows.push(
        json!({"op":op,"handle":handle,"arg":arg,"error":error,"reply":result,
        "cycles":s.cycles-start,"memory_transactions":memory.borrow().transfers-transfers}),
    );
    result
}

pub fn boundaries() -> Result<Value> {
    let mut cases = Vec::new();
    for (views, events, retired) in [(1, 1, 1), (3, 3, 1), (5, 3, 3)] {
        let limits = StoreLimits {
            views,
            max_events: events,
            block_events: 1,
            blocks: views * events,
            retirements: retired,
        };
        let mut design = Design::new(
            "midi_store",
            vec![crate::root().join("fpga/midi/rtl/midi_store.sv")],
        );
        design.includes.push(crate::root().join("fpga/midi/rtl"));
        let compiled = design
            .parameters(&[
                ("VIEWS", u64::from(views)),
                ("MAX_EVENTS", u64::from(events)),
                ("BLOCK_EVENTS", 1),
                ("BLOCKS", u64::from(limits.blocks)),
                ("RETIREMENTS", u64::from(retired)),
            ])
            .build(&crate::root().join("work/marlin-suite/midi-store-area"))?;
        let mut s = Sim::new(compiled.benchmark_session()?, &[("clk", 10000, 0)]);
        let memory = Rc::new(RefCell::new(Memory::new(
            (s.model.get("arena_bytes") / 4) as usize,
        )));
        s.agent(memory.clone());
        s.agent(crate::protocol::ReadyValid::new(
            "memory_valid",
            "memory_ready",
            &["memory_address", "memory_write", "memory_write_data"],
        ));
        s.agent(crate::protocol::ReadyValid::new(
            "response_valid",
            "response_ready",
            &[
                "response_error",
                "response_handle",
                "response_frame",
                "response_order",
                "response_packet",
                "response_length",
                "response_work",
                "response_events",
                "response_status",
                "response_blocks",
                "response_owner",
                "response_references",
            ],
        ));
        s.model.set_all(&[
            ("reset", 1),
            ("command_valid", 0),
            ("response_ready", 0),
            ("arena_base", 0),
        ]);
        s.idle(2);
        s.model.set("reset", 0);
        let mut rows = Vec::new();
        command(&mut s, &memory, &mut rows, 4, 0, 0, 0, 4);
        command(&mut s, &memory, &mut rows, 0, 0, 0, 0, 0);
        let mut reference = ContentStore::prepare(limits).unwrap();
        let note = Message::parse(&[0x90, 60, 99]).unwrap();
        let packet = 0x633c90;
        let frame_base = 1u64 << 40;
        let mut old_handles = Vec::new();
        for round in 0..3 {
            let mut owners = VecDeque::new();
            for _ in 0..views {
                let logical = reference.writer().unwrap();
                let handle = command(&mut s, &memory, &mut rows, 4, 0, 0, 0, 0)["handle"]
                    .as_u64()
                    .unwrap();
                assert!(!old_handles.contains(&handle));
                for index in 0..events {
                    reference
                        .append(logical, frame_base + u64::from(index), note)
                        .unwrap();
                    command(
                        &mut s,
                        &memory,
                        &mut rows,
                        5,
                        handle,
                        frame_base + u64::from(index),
                        packet,
                        0,
                    );
                }
                if round == 1 {
                    assert_eq!(
                        reference.append(logical, frame_base + u64::from(events), note),
                        Err(Error::Capacity)
                    );
                    command(
                        &mut s,
                        &memory,
                        &mut rows,
                        5,
                        handle,
                        frame_base + u64::from(events),
                        packet,
                        3,
                    );
                    assert_eq!(
                        reference.finish(logical, frame_base + u64::from(events) + 1),
                        Err(Error::Incomplete)
                    );
                    command(
                        &mut s,
                        &memory,
                        &mut rows,
                        6,
                        handle,
                        frame_base + u64::from(events) + 1,
                        0,
                        8,
                    );
                } else {
                    reference
                        .finish(logical, frame_base + u64::from(events) + 1)
                        .unwrap();
                    command(
                        &mut s,
                        &memory,
                        &mut rows,
                        6,
                        handle,
                        frame_base + u64::from(events) + 1,
                        0,
                        0,
                    );
                    reference.retain(logical).unwrap();
                    command(&mut s, &memory, &mut rows, 8, handle, 0, 0, 0);
                    reference.release(logical).unwrap();
                    command(&mut s, &memory, &mut rows, 9, handle, 0, 0, 0);
                }
                for index in 0..events {
                    let record = reference.read(logical, index).unwrap().unwrap();
                    let actual = command(
                        &mut s,
                        &memory,
                        &mut rows,
                        10,
                        handle,
                        u64::from(index),
                        0,
                        0,
                    );
                    assert_eq!(
                        (
                            actual["frame"].as_u64().unwrap(),
                            actual["packet"].as_u64().unwrap(),
                            actual["length"].as_u64().unwrap(),
                            actual["order"].as_u64().unwrap()
                        ),
                        (record.frame, packet, 3, u64::from(record.order))
                    );
                }
                let actual = command(&mut s, &memory, &mut rows, 10, handle, u64::MAX, 0, 0);
                assert_eq!(actual["length"], 0);
                owners.push_back((logical, handle));
            }
            assert_eq!(reference.writer(), Err(Error::Capacity));
            command(&mut s, &memory, &mut rows, 4, 0, 0, 0, 3);
            for handle in [0, 0x1_80000000, u64::MAX] {
                command(&mut s, &memory, &mut rows, 12, handle, 0, 0, 4);
            }
            while !owners.is_empty() || reference.usage().retirements != 0 {
                while let Some(&(logical, handle)) = owners.front() {
                    match reference.release(logical) {
                        Ok(()) => {
                            command(&mut s, &memory, &mut rows, 9, handle, 0, 0, 0);
                            old_handles.push(handle);
                            owners.pop_front();
                        }
                        Err(Error::Capacity) => {
                            command(&mut s, &memory, &mut rows, 9, handle, 0, 0, 3);
                            break;
                        }
                        error => panic!("Unexpected release: {error:?}"),
                    }
                }
                let work = reference.maintenance(1);
                let actual = command(&mut s, &memory, &mut rows, 11, 0, 1, 0, 0);
                assert_eq!(actual["work"], work);
                let usage = reference.usage();
                assert_eq!(
                    s.model.get("free_block_slots"),
                    u64::from(limits.blocks) - usage.blocks as u64
                );
                assert_eq!(
                    s.model.get("free_view_slots"),
                    u64::from(views) - usage.views as u64
                );
                assert_eq!(
                    s.model.get("free_retirement_slots"),
                    u64::from(retired) - usage.retirements as u64
                );
            }
            for &handle in &old_handles {
                command(&mut s, &memory, &mut rows, 12, handle, 0, 0, 4);
            }
        }
        memory.borrow_mut().words[0] = u32::MAX;
        command(&mut s, &memory, &mut rows, 4, 0, 0, 0, 6);
        memory.borrow_mut().fail = Some(0);
        command(&mut s, &memory, &mut rows, 0, 0, 0, 0, 7);
        memory.borrow_mut().fail = None;
        s.model.set("reset", 1);
        s.idle(1);
        s.model.set("reset", 0);
        command(&mut s, &memory, &mut rows, 4, 0, 0, 0, 4);
        command(&mut s, &memory, &mut rows, 0, 0, 0, 0, 0);
        command(&mut s, &memory, &mut rows, 4, 0, 0, 0, 0);
        cases.push(json!({"views":views,"blocks":limits.blocks,"events_per_view":events,"retirements":retired,"measurements":rows}));
    }
    Ok(
        json!({"scope":"standalone MIDI store, random word-memory backpressure; minimum/odd capacities and full pools over three generation/retirement wraps",
        "validation":"exact ContentStore record/frame/packet and usage agreement; high-bit invalid handles, max-plus-one refusal, generation overflow, poisoned accepted command inputs, held responses, memory faults and stale-RAM reset","cases":cases}),
    )
}
