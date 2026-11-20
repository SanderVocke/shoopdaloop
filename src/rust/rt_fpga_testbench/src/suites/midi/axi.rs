use super::{command, runtime::Sink};
use crate::{
    Result,
    memory::{Backend, Memory, Mode},
    model::Design,
    sim::Sim,
};
use rt_fpga_reference::midi::{Message, OutputEvent};
use std::{cell::RefCell, rc::Rc};

fn issue(s: &mut Sim<'_>, op: u64, args: [u64; 5]) -> u64 {
    s.model.set("command_op", op);
    for (i, value) in args.into_iter().enumerate() {
        s.model.set(&format!("command_arg{i}"), value);
    }
    command(s)
}
pub fn axi_conformance(width: u64) -> Result<()> {
    let sources = ["midi_job_service_axi", "midi_job_service", "midi_store"];
    let mut paths: Vec<_> = sources
        .iter()
        .map(|p| crate::root().join(format!("fpga/midi/rtl/{p}.sv")))
        .collect();
    paths.extend(
        ["word_axi", "word_dma", "axi_master"]
            .iter()
            .map(|p| crate::root().join(format!("fpga/axi_memory/rtl/{p}.sv"))),
    );
    let mut d = Design::new("midi_job_service_axi", paths);
    d.includes.push(crate::root().join("fpga/midi/rtl"));
    let compiled = d
        .parameters(&[
            ("DATA_WIDTH", width),
            ("LOOPS", 2),
            ("INPUTS", 1),
            ("DESTINATIONS", 1),
            ("BLOCK_EVENTS", 4),
            ("BLOCKS", 32),
            ("MAX_EVENTS", 16),
            ("VIEWS", 8),
            ("RETIREMENTS", 8),
            ("INPUT_EVENTS", 4),
            ("MAINTENANCE", 4),
        ])
        .build(&crate::root().join("work/marlin-suite/midi"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
    s.max_ps = 1_000_000_000_000_000;
    const LOGICAL: u64 = 0x100000000;
    const PHYSICAL: u64 = 0x10000;
    let size = s.model.get("arena_bytes") + 1024;
    let memory = Memory::attach(
        &mut s,
        Mode::Fixed,
        size as usize / 4,
        PHYSICAL,
        &crate::artifact_directory("midi"),
        119,
    )?;
    memory.borrow_mut().control.stalls = true;
    let sink = Rc::new(RefCell::new(Sink::new()));
    s.agent(sink.clone());
    s.model.set_all(&[
        ("reset", 1),
        ("quantum_frames", 16),
        ("arena_base", LOGICAL),
        ("physical_base", PHYSICAL),
        ("memory_arena_bytes", size),
    ]);
    s.idle(2);
    s.model.set("reset", 0);
    assert_eq!(issue(&mut s, 0, [0; 5]), 0);
    assert_eq!(issue(&mut s, 1, [0, 1 << 32, 0, 0, 1]), 0);
    let event_offset = size - 512;
    if let Backend::Axi(a) = &mut memory.borrow_mut().backend {
        a.words[event_offset as usize / 4..event_offset as usize / 4 + 8]
            .copy_from_slice(&[0, 1, 0x03503c90, 0, 0, 15, 0x03003c80, 0]);
    }
    let quantum = |s: &mut Sim<'_>, start, mode: u64, begin, position, length, count| {
        let identity = 1 << 32;
        assert_eq!(
            issue(s, 24, [start + 16, LOGICAL + event_offset, count, 0, 0]),
            0
        );
        if mode != 2 {
            assert_eq!(issue(s, 25, [0, 0, 0, 0, identity]), 0);
        }
        if begin {
            assert_eq!(issue(s, 26, [0, start, 0, 0, identity]), 0);
        }
        assert_eq!(issue(s, 27, [0; 5]), 0);
        if mode == 2 {
            assert_eq!(issue(s, 28, [0, 0, 0, 0, identity]), 0);
        }
        assert_eq!(
            issue(
                s,
                29,
                [0, position, length, mode | u64::from(begin) << 2, identity]
            ),
            0
        );
        if mode == 1 {
            assert_eq!(issue(s, 30, [0, 3, 0, 0, identity]), 0);
            assert_eq!(issue(s, 30, [0, 1, 0, 0, identity]), 0);
            assert_eq!(issue(s, 31, [0, position, length, mode, identity]), 0);
        }
        assert_eq!(issue(s, 32, [0; 5]), 0);
        assert_eq!(issue(s, 33, [0, mode, 0, 0, identity]), 0);
        assert_eq!(issue(s, 34, [0; 5]), 0);
    };
    quantum(&mut s, 0, 2, true, 0, 0, 2);
    quantum(&mut s, 16, 0, false, 0, 16, 0);
    quantum(&mut s, 32, 1, false, 0, 16, 0);
    assert_eq!(
        sink.borrow().messages(),
        [
            OutputEvent {
                destination: 0,
                offset: 1,
                message: Message::managed(&[0x90, 60, 80]).unwrap()
            },
            OutputEvent {
                destination: 0,
                offset: 15,
                message: Message::managed(&[0x80, 60, 0]).unwrap()
            }
        ]
    );
    assert_eq!(issue(&mut s, 19, [1, 0, 0, 0, 0]), 2);
    if let Backend::Axi(a) = &memory.borrow().backend {
        assert!(a.reads > 0 && a.writes > 0);
        assert!(a.records.iter().all(|r| r.length == 0));
    }
    memory.borrow_mut().control.fail_next = true;
    assert_eq!(issue(&mut s, 11, [0, 0, 0, 0, 0]), 7);
    assert_eq!(issue(&mut s, 19, [0, 0, 0, 0, 0]), 4);
    let reads = memory.borrow().reads();
    let writes = memory.borrow().writes();
    assert_eq!(issue(&mut s, 17, [0; 5]), 4);
    assert_eq!(sink.borrow().messages().len(), 2);
    assert_eq!(memory.borrow().reads(), reads);
    assert_eq!(memory.borrow().writes(), writes);
    s.model.set("reset", 1);
    s.idle(2);
    s.model.set("reset", 0);
    s.model.set("memory_arena_bytes", 4);
    assert_eq!(issue(&mut s, 0, [0; 5]), 7);
    Ok(())
}
