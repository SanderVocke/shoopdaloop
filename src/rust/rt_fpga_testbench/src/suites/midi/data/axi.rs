use super::*;
use crate::{
    memory::{Backend, Memory as AxiMemory, Mode},
    model::Design,
};
use rt_fpga_reference::midi;

pub fn conformance(width: u64) -> Result<()> {
    let mut sources: Vec<_> = ["midi_data_jobs_axi", "midi_data_jobs", "midi_store"]
        .iter()
        .map(|n| crate::root().join(format!("fpga/midi/rtl/{n}.sv")))
        .collect();
    sources.extend(
        ["word_axi", "word_dma", "axi_master"]
            .iter()
            .map(|n| crate::root().join(format!("fpga/axi_memory/rtl/{n}.sv"))),
    );
    let mut design = Design::new("midi_data_jobs_axi", sources);
    design.includes.push(crate::root().join("fpga/midi/rtl"));
    let compiled = design
        .parameters(&[
            ("DATA_WIDTH", width),
            ("LOOPS", 2),
            ("INPUTS", 2),
            ("DESTINATIONS", 2),
            ("BUFFERS", 8),
            ("CURSORS", 2),
            ("SCRATCH_EVENTS", 4096),
            ("BLOCK_EVENTS", 8),
            ("BLOCKS", 128),
            ("MAX_EVENTS", 1024),
            ("VIEWS", 16),
            ("RETIREMENTS", 16),
        ])
        .build(&crate::root().join("work/marlin-suite/midi-data-axi"))?;
    let mut sim = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
    sim.max_ps = 1_000_000_000_000_000;
    let address = (sim.model.get("arena_bytes") + 271) & !15;
    let size = address + 16384;
    const BASE: u64 = 0x100000000;
    let memory = AxiMemory::attach(
        &mut sim,
        Mode::Fixed,
        size as usize / 4,
        0x10000,
        &crate::artifact_directory("midi-data-axi"),
        119,
    )?;
    memory.borrow_mut().control.stalls = true;
    let sink = Rc::new(RefCell::new(Sink::default()));
    sim.agent(sink.clone());
    sim.agent(crate::protocol::ReadyValid::new(
        "packet_valid",
        "packet_ready",
        &[
            "packet_data",
            "packet_length",
            "packet_offset",
            "packet_destination",
        ],
    ));
    sim.model.set_all(&[
        ("reset", 1),
        ("control_enable", 1),
        ("quantum_frames", 32),
        ("arena_base", BASE),
        ("memory_arena_bytes", size),
        ("physical_base", 0x10000),
    ]);
    sim.idle(2);
    sim.model.set("reset", 0);
    assert_eq!(issue_control(&mut sim, 0, [0; 5]).0, 0);
    let mut f = Oracle::new(32, false);
    let events: Vec<_> = (0..300)
        .flat_map(|i| {
            [
                event(1, 2, &[0xb1, 6, (i % 128) as u8]),
                event(0, 2, &[0xb0, 64, (i % 128) as u8]),
            ]
        })
        .collect();
    for tick in 0..4 {
        if tick == 0 {
            for slot in 0..2 {
                f.command(slot, Operation::Record);
            }
        }
        if tick == 1 {
            for slot in 0..2 {
                f.command(
                    slot,
                    Operation::Play {
                        position: 0,
                        retrigger: true,
                    },
                );
            }
        }
        if tick == 3 {
            for slot in 0..2 {
                f.command(slot, Operation::Stop);
            }
        }
        let input = if tick == 0 { events.as_slice() } else { &[] };
        if let Backend::Axi(a) = &mut memory.borrow_mut().backend {
            for (i, e) in input.iter().enumerate() {
                a.words[address as usize / 4 + 4 * i..address as usize / 4 + 4 * i + 4]
                    .copy_from_slice(&[u32::from(e.input), e.offset, packed(e.message), 0]);
            }
        }
        sink.borrow_mut().events.clear();
        assert_eq!(
            prepare_core(&mut sim, BASE + address, input.len() as u64),
            0
        );
        for job in f.run(input) {
            assert_eq!(issue_job(&mut sim, job), 0, "{job:?}");
        }
        issue_finish(&mut sim);
        assert_eq!(sink.borrow().events, f.emitted);
    }
    assert_eq!(prepare_core(&mut sim, BASE + address, 0), 0);
    memory.borrow_mut().control.fail_next = true;
    let read = Job::MidiReadView {
        owner: id(0),
        sequence: midi::SequenceId(0),
        cursor: midi::CursorId(0),
        position: 0,
        frames: 32,
        restart: true,
        channels: u16::MAX,
        destination: midi::BufferId(0),
    };
    assert_eq!(issue_job(&mut sim, read), 7);
    assert_eq!(prepare_core(&mut sim, BASE + address, 0), 4);
    Ok(())
}
