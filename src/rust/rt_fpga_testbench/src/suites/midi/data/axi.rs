use super::*;
use crate::{
    memory::{Backend, Memory as AxiMemory, Mode},
    model::Design,
};
use rt_fpga_reference::midi;

pub fn conformance(width: u64) -> Result<()> {
    conformance_config(width, false)
}
pub fn expression_conformance(width: u64) -> Result<()> {
    conformance_config(width, true)
}
fn conformance_config(width: u64, expression: bool) -> Result<()> {
    let mut sources: Vec<_> = [
        "midi_data_jobs_axi",
        "midi_data_jobs",
        "midi_store",
        "midi_expression_state",
    ]
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
            ("STATE_SUPPORTED", u64::from(expression)),
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
    let mut f = Oracle::build(32, false, expression);
    if expression {
        for n in 0..2u64 {
            assert_eq!(
                issue_control(
                    &mut sim,
                    21,
                    [n, (1 << 32) | n, n | (1 << 8) | ((1 << n) << 16), 0, 0]
                )
                .0,
                0
            );
            let mut source = midi::performance::State::new();
            for bytes in [
                [0xb0 | n as u8, 1, 20],
                [0x90 | n as u8, 60, 80],
                [0xb0 | n as u8, 66, 127],
            ] {
                source.apply(midi::Message::parse(&bytes).unwrap());
            }
            f.midi
                .expression
                .as_mut()
                .unwrap()
                .seed_input(n as u16, &source)
                .unwrap();
            for field in [
                n as usize * 512 + 60,
                n as usize * 512 + 129,
                n as usize * 512 + 194,
            ] {
                assert_eq!(
                    issue_control(
                        &mut sim,
                        22,
                        [0, n, field as u64, u64::from(source.word(field)), 0]
                    )
                    .0,
                    0
                );
            }
        }
    }
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

pub fn compiled_pipeline(width: u64, delay: u64) -> Result<()> {
    use super::pipeline::{CAP, geometry, update_plans};
    use crate::{
        drivers::{audio::Audio, store::Store},
        memory::{Control, axi::Axi},
        timing::Fixed,
    };
    let mut design =
        super::pipeline::design(32).parameters(&[("DDR_PROGRAM", 1), ("AXI_DATA_WIDTH", width)]);
    design.top = "midi_audio_pipeline_axi".into();
    design.memory = None;
    design
        .sources
        .push(crate::root().join("fpga/midi/rtl/midi_audio_pipeline_axi.sv"));
    design
        .parameters
        .insert("MIDI_AXI_DATA_WIDTH".into(), width.to_string());
    let compiled = design.build(&crate::root().join("work/marlin-suite/midi-connected-axi"))?;
    let mut sim = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
    sim.max_ps = 1_000_000_000_000_000;
    let address = (sim.model.get("midi_arena_bytes") + 271) & !15;
    let words = (address / 4 + 8192) as usize;
    let memory = Rc::new(RefCell::new(AxiMemory {
        control: Control {
            stalls: true,
            ..Default::default()
        },
        backend: Backend::Axi(Box::new(
            Axi::new(
                words,
                0x30000000,
                width as usize / 8,
                Box::new(Fixed::new(delay)),
                5001,
            )
            .with_prefix("midi_"),
        )),
        words,
        base: 0x30000000,
        width: width as usize / 8,
        sample_words: None,
        sample_transactions: 0,
    }));
    sim.agent(memory.clone());
    let sink = Rc::new(RefCell::new(Sink::mixed()));
    sim.agent(sink.clone());
    sim.agent(crate::protocol::ReadyValid::new(
        "midi_packet_valid",
        "midi_packet_ready",
        &[
            "midi_packet_data",
            "midi_packet_length",
            "midi_packet_offset",
            "midi_packet_destination",
        ],
    ));
    let mut audio = Audio::new(&mut sim, CAP, 601);
    let mut store = Store::new(&mut sim, geometry(32), Mode::Fixed, 617, "store_")?;
    store.reserve_ddr_program(CAP[0] as usize, 1);
    if let Backend::Axi(memory) = &mut store.memory.borrow_mut().backend {
        memory.timing = Box::new(Fixed::new(delay));
    }
    let mut oracle = Oracle::new(32, true);
    let image = crate::target::RtlTarget::new(CAP, 16, &[0, 1]).build(&logical(32, true))?;
    audio.load_compiled(&mut sim, &image);
    store.load(
        &mut sim,
        &serde_json::to_value(oracle.audio.snapshot()).unwrap(),
    );
    const BASE: u64 = 0x100000000;
    sim.model.set_all(&[
        ("sync_event_ready", 1),
        ("run_quantum", 32),
        ("midi_arena_base", BASE),
        ("midi_memory_arena_bytes", (words * 4) as u64),
        ("midi_command_op", 0),
        ("midi_command_valid", 1),
        ("midi_response_ready", 0),
    ]);
    sim.until(100, "MIDI format admission", |m| {
        m.get("midi_command_ready") != 0
    });
    sim.idle(1);
    sim.model.set("midi_command_valid", 0);
    sim.until(2_000_000, "MIDI format reply", |m| {
        m.get("midi_response_valid") != 0
    });
    assert_eq!(sim.model.get("midi_response_error"), 0);
    sim.model.set("midi_response_ready", 1);
    sim.idle(1);
    sim.model.set("midi_response_ready", 0);
    let input: Vec<_> = (0..200)
        .flat_map(|n| {
            [
                event(1, 2, &[0xb1, 1, (n % 128) as u8]),
                event(0, 2, &[0xb0, 1, (n % 128) as u8]),
            ]
        })
        .collect();
    for tick in 0..4 {
        if tick == 0 {
            for slot in 0..2 {
                oracle.command(slot, Operation::Record);
            }
        }
        if tick == 1 {
            for slot in 0..2 {
                oracle.command(
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
                oracle.command(slot, Operation::Stop);
            }
        }
        update_plans(&mut audio, &mut sim, &oracle);
        let events = if tick == 0 { input.as_slice() } else { &[] };
        if let Backend::Axi(a) = &mut memory.borrow_mut().backend {
            for (i, e) in events.iter().enumerate() {
                a.words[address as usize / 4 + 4 * i..address as usize / 4 + 4 * i + 4]
                    .copy_from_slice(&[u32::from(e.input), e.offset, packed(e.message), 0]);
            }
        }
        sim.model.set_all(&[
            ("midi_event_address", BASE + address),
            ("midi_event_count", events.len() as u64),
        ]);
        sink.borrow_mut().events.clear();
        let jobs = oracle.run(events);
        assert_eq!(
            audio.execute_with_limit(&mut sim, &[vec![1 << 22; 32]], true, 5_000_000),
            (2, 0)
        );
        assert_scheduled_jobs(audio.consumed(), &jobs);
        assert_eq!(sink.borrow().events, oracle.emitted);
        assert_eq!(
            audio.read_samples(&mut sim).1,
            vec![vec![if tick == 1 || tick == 2 { 1 << 22 } else { 0 }; 32]; 2]
        );
    }
    Ok(())
}
