use super::{Memory, projection};
use crate::{
    Result,
    clock::Edges,
    drivers::{audio::Audio, store::Geometry},
    model::Model,
    sim::{Agent, Sim},
};
use rt_fpga_compiler::StateProgramTarget;
use rt_fpga_reference::{
    midi::{InputEvent, Message, OutputEvent},
    transitions::Operation,
};
use std::{cell::RefCell, rc::Rc};
#[path = "../../../../rt_fpga_reference/tests/support/midi_session.rs"]
#[allow(dead_code, unused_imports)]
mod reference;

struct Output {
    events: Vec<OutputEvent>,
    remaining: usize,
}
impl Agent for Output {
    fn drive(&mut self, m: &mut Model<'_>, e: Edges) {
        if e.rises(0) {
            m.set("midi_packet_ready", 1);
            m.set("midi_packet_refused", u64::from(self.remaining == 0));
        }
    }
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if e.rises(0)
            && m.get("midi_packet_valid") != 0
            && m.get("midi_packet_ready") != 0
            && m.get("midi_packet_refused") == 0
        {
            let bytes = (m.get("midi_packet_data") as u32).to_le_bytes();
            self.events.push(OutputEvent {
                destination: m.get("midi_packet_destination") as u16,
                offset: m.get("midi_packet_offset") as u32,
                message: Message::managed(&bytes[..m.get("midi_packet_length") as usize]).unwrap(),
            });
            self.remaining -= 1;
        }
    }
}
fn control(s: &mut Sim<'_>, op: u64, args: [u64; 5]) -> u64 {
    s.model.set("midi_command_op", op);
    for (i, value) in args.into_iter().enumerate() {
        s.model.set(&format!("midi_command_arg{i}"), value);
    }
    s.model
        .set_all(&[("midi_command_valid", 1), ("midi_response_ready", 0)]);
    s.until(20_000_000, "MIDI control admission", |m| {
        m.get("midi_command_ready") != 0
    });
    s.idle(1);
    s.model.set("midi_command_valid", 0);
    s.until(20_000_000, "MIDI control response", |m| {
        m.get("midi_response_valid") != 0
    });
    let error = s.model.get("midi_response_error");
    s.model.set("midi_response_ready", 1);
    s.idle(1);
    error
}
fn write_events(memory: &mut Memory, address: usize, events: &[InputEvent]) {
    for (n, event) in events.iter().enumerate() {
        let packet = event
            .message
            .data()
            .iter()
            .enumerate()
            .fold((event.message.data().len() as u32) << 24, |v, (i, b)| {
                v | (u32::from(*b) << (8 * i))
            });
        memory.words[address / 4 + 4 * n..address / 4 + 4 * n + 4].copy_from_slice(&[
            u32::from(event.input),
            event.offset,
            packet,
            0,
        ]);
    }
}

fn update_row(audio: &mut Audio, s: &mut Sim<'_>, f: &reference::Fixture, i: usize, all: bool) {
    let slot = f.pipeline.state().loop_state(reference::id(i)).unwrap();
    let row = rt_fpga_reference::transitions::LoopSpec {
        id: slot.state.id,
        kind: rt_fpga_reference::transitions::LoopKind::Primitive,
        length: 0,
        sync_source: None,
        repeat_sync: false,
        timeline: None,
    };
    let image = crate::loop_image::image(slot, &row, None);
    if all {
        crate::loop_image::write_words(&mut audio.program, s, i as u32, &image, 0, 16);
    } else {
        crate::loop_image::write_plans(&mut audio.program, s, i as u32, &image);
    }
}
fn quantum(
    audio: &mut Audio,
    s: &mut Sim<'_>,
    f: &mut reference::Fixture,
    memory: &Rc<RefCell<Memory>>,
    output: &Rc<RefCell<Output>>,
    address: usize,
    events: &[InputEvent],
) {
    for i in 0..2 {
        update_row(audio, s, f, i, false);
    }
    write_events(&mut memory.borrow_mut(), address, events);
    s.model.set_all(&[
        ("midi_event_address", address as u64),
        ("midi_event_count", events.len() as u64),
    ]);
    output.borrow_mut().remaining = f.capacity;
    let report = f.run(events);
    let error = match report.error {
        None => 0,
        Some(reference::Error::InvalidEvent) => 1,
        Some(reference::Error::Unsupported) => 2,
        Some(reference::Error::Capacity) => 3,
        Some(reference::Error::InvalidState) => 4,
        Some(reference::Error::Busy) => 5,
        Some(reference::Error::Overflow) => 6,
        Some(reference::Error::Incomplete) => 8,
    };
    let start = s.cycles;
    assert_eq!(
        audio.execute_with_limit(s, &[vec![17; f.q as usize]], true, 20_000_000),
        (2, 0)
    );
    assert_eq!(s.model.get("midi_error_code"), error, "Q={}", f.q);
    assert_eq!(audio.read_samples(s).1, vec![vec![17; f.q as usize]]);
    assert_eq!(
        audio.consumed(),
        f.pipeline
            .emitted_jobs()
            .iter()
            .map(|j| serde_json::to_value(j).unwrap())
            .collect::<Vec<_>>(),
        "Q={}",
        f.q
    );
    assert_eq!(
        std::mem::take(&mut output.borrow_mut().events),
        std::mem::take(&mut f.emitted),
        "Q={}",
        f.q
    );
    for destination in 0..2 {
        match f.midi.output_state(destination as u16) {
            Ok(state) => {
                assert_eq!(control(s, 19, [1, destination, 0, 0, 0]), 0);
                let at = s.model.get("midi_response_state") as usize / 4;
                assert_eq!(&memory.borrow().words[at..at + 4225], projection(state));
            }
            Err(reference::Error::Busy) => assert_eq!(control(s, 19, [1, destination, 0, 0, 0]), 5),
            Err(error) => panic!("unexpected reference state: {error:?}"),
        }
    }
    println!("mixed Q={} cycles={}", f.q, s.cycles - start);
}
fn publish(audio: &mut Audio, s: &mut Sim<'_>, f: &reference::Fixture, op: u64, args: [u64; 5]) {
    s.model.set("midi_command_op", op);
    for (i, value) in args.into_iter().enumerate() {
        s.model.set(&format!("midi_command_arg{i}"), value);
    }
    s.model
        .set_all(&[("midi_command_valid", 1), ("midi_response_ready", 0)]);
    s.until(20_000_000, "publication admission", |m| {
        m.get("midi_command_ready") != 0
    });
    s.idle(1);
    s.model.set("midi_command_valid", 0);
    s.until(20_000_000, "publication prepared", |m| {
        m.get("midi_publication_valid") != 0 || m.get("midi_response_valid") != 0
    });
    assert_eq!(s.model.get("midi_publication_valid"), 1);
    let frame = s.model.get("sync_frame");
    s.idle(17);
    assert_eq!(s.model.get("sync_frame"), frame);
    assert_eq!(s.model.get("run_ready"), 0);
    assert_eq!(s.model.get("midi_response_valid"), 0);
    for i in 0..2 {
        update_row(audio, s, f, i, true);
    }
    s.model.set("midi_publication_commit", 1);
    s.idle(1);
    s.model.set("midi_publication_commit", 0);
    s.until(20_000_000, "publication completion", |m| {
        m.get("midi_response_valid") != 0
    });
    assert_eq!(s.model.get("midi_response_error"), 0);
    s.model.set("midi_response_ready", 1);
    s.idle(1);
}

pub fn compiled_mixed_conformance() -> Result<()> {
    let cap = [64, 8, 64, 128, 4, 1, 1];
    let geometry = Geometry::new([4, 64, 1], 8);
    let mut design =
        crate::suites::pipeline::design(cap, &geometry, 64, crate::memory::Mode::Instant, 1);
    design.top = "midi_audio_pipeline".into();
    design.probes.clear();
    design.includes.push(crate::root().join("fpga/midi/rtl"));
    for name in [
        "midi_audio_pipeline",
        "midi_job_port",
        "midi_job_service",
        "midi_store",
        "midi_state_service",
        "midi_state",
        "midi_difference",
        "midi_transition",
    ] {
        design
            .sources
            .push(crate::root().join(format!("fpga/midi/rtl/{name}.sv")));
    }
    for (name, value) in [
        ("MIDI_LOOPS", 2),
        ("MIDI_INPUTS", 1),
        ("MIDI_DESTINATIONS", 2),
        ("MIDI_BLOCK_EVENTS", 8),
        ("MIDI_BLOCKS", 128),
        ("MIDI_MAX_EVENTS", 128),
        ("MIDI_VIEWS", 64),
        ("MIDI_SNAPSHOTS", 64),
        ("MIDI_RETIREMENTS", 32),
        ("MIDI_INPUT_EVENTS", 16),
        ("MIDI_MAINTENANCE", 8),
    ] {
        design.parameters.insert(name.into(), value.to_string());
    }
    let compiled = design.build(&crate::root().join("work/marlin-suite/midi-mixed"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
    s.max_ps = 1_000_000_000_000_000;
    let address = s.model.get("midi_arena_bytes") as usize + 256;
    let memory = Rc::new(RefCell::new(Memory::mixed(address / 4 + 8192)));
    let output = Rc::new(RefCell::new(Output {
        events: Vec::new(),
        remaining: usize::MAX,
    }));
    s.agent(memory.clone());
    s.agent(output.clone());
    s.agent(crate::protocol::ReadyValid::new(
        "midi_memory_valid",
        "midi_memory_ready",
        &[
            "midi_memory_address",
            "midi_memory_write",
            "midi_memory_write_data",
        ],
    ));
    s.agent(crate::protocol::ReadyValid::new(
        "midi_packet_valid",
        "midi_packet_ready",
        &[
            "midi_packet_data",
            "midi_packet_length",
            "midi_packet_offset",
            "midi_packet_destination",
        ],
    ));
    let mut audio = Audio::new(&mut s, cap, 1401);
    for q in [1, 16, 32, 64, 128] {
        let mut f = reference::Fixture::new(q, 2);
        f.pipeline.trace_jobs(true);
        let logical = reference::logical(q, reference::limits(2));
        let rtl = crate::target::RtlTarget::new(cap, 8, &[]).build(&logical)?;
        audio.load_compiled(&mut s, &rtl);
        s.model.set("sync_event_ready", 1);
        assert_eq!(control(&mut s, 0, [0; 5]), 0);
        assert_eq!(control(&mut s, 16, [0, 128, 0, 0, 0]), 0);
        for i in 0..2 {
            assert_eq!(control(&mut s, 1, [i, i | (1 << 32), 0, 0, 1 << i]), 0);
        }
        for tick in 0..9 {
            let operation = match tick {
                0 => Some(Operation::Record),
                2 | 5 => Some(Operation::Play {
                    position: 0,
                    retrigger: true,
                }),
                4 | 8 => Some(Operation::Stop),
                _ => None,
            };
            if let Some(operation) = operation {
                for i in 0..2 {
                    f.command(i, operation);
                }
            }
            let events = if tick < 2 {
                vec![
                    reference::event(0, &[0x90, 60, 80]),
                    reference::event(q - 1, &[0x91, 61, 90]),
                ]
            } else {
                vec![]
            };
            quantum(
                &mut audio, &mut s, &mut f, &memory, &output, address, &events,
            );
        }
        if q == 32 {
            for i in 0..2 {
                f.command(
                    i,
                    Operation::Play {
                        position: 0,
                        retrigger: true,
                    },
                );
            }
            f.capacity = 1;
            quantum(&mut audio, &mut s, &mut f, &memory, &output, address, &[]);
            for i in 0..2 {
                f.command(i, Operation::Stop);
            }
            quantum(&mut audio, &mut s, &mut f, &memory, &output, address, &[]);
            f.capacity = usize::MAX;
            quantum(&mut audio, &mut s, &mut f, &memory, &output, address, &[]);
            let end = f.pipeline.state().frame();
            f.pipeline
                .grab_midi(&mut f.midi, 0, end - u64::from(q), end, &[0, 1])
                .unwrap();
            memory.borrow_mut().words[address / 4 + 128..address / 4 + 130]
                .copy_from_slice(&[0, 1]);
            publish(
                &mut audio,
                &mut s,
                &f,
                4,
                [0, end - u64::from(q), end, address as u64 + 512, 2],
            );
            for i in 0..2 {
                f.command(
                    i,
                    Operation::Play {
                        position: 0,
                        retrigger: true,
                    },
                );
            }
            quantum(&mut audio, &mut s, &mut f, &memory, &output, address, &[]);
            for i in 0..2 {
                f.command(i, Operation::Stop);
            }
            quantum(&mut audio, &mut s, &mut f, &memory, &output, address, &[]);
            f.pipeline.clear_midi(&mut f.midi, &[0, 1]).unwrap();
            publish(
                &mut audio,
                &mut s,
                &f,
                22,
                [address as u64 + 512, 2, 0, 0, 0],
            );
            quantum(&mut audio, &mut s, &mut f, &memory, &output, address, &[]);
            assert_eq!(
                audio.execute_with_limit(&mut s, &[], false, 20_000_000),
                (2, 1)
            );
            let mut silence = vec![reference::Fixed::ZERO; q as usize];
            let failed = f.pipeline.process_midi_session(
                &mut f.jobs,
                &mut f.audio,
                &[],
                &mut [&mut silence],
                q as usize,
                &mut f.midi,
                &[],
                &mut |_| panic!("MIDI ran after an audio fault"),
            );
            assert!(matches!(failed, Err(reference::ProcessError::Pipeline(_))));
            assert_eq!(s.model.get("sync_frame"), f.pipeline.state().frame());
            f.midi
                .drain_recovery(0, &mut |e| {
                    f.emitted.push(e);
                    Ok(())
                })
                .unwrap();
            assert_eq!(control(&mut s, 17, [0; 5]), 0);
            assert_eq!(
                std::mem::take(&mut output.borrow_mut().events),
                std::mem::take(&mut f.emitted)
            );
        }
    }
    Ok(())
}
