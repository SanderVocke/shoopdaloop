//! Compiled SP/common-JP integration checks.
use super::super::Memory;
use super::{Oracle, Sink, logical, write_events};
use crate::{
    Result,
    drivers::{
        audio::Audio,
        store::{Geometry, Store},
    },
    memory::Mode,
    model::{Compiled, Design},
    sim::Sim,
};
use rt_fpga_compiler::StateProgramTarget;
use rt_fpga_reference::{
    midi::{InputEvent, Message, SequenceId, ViewId},
    transitions::Operation,
};
use std::{cell::RefCell, rc::Rc};
pub(super) const CAP: [u64; 7] = [64, 16, 64, 128, 8, 2, 2];
pub(super) fn geometry(q: u64) -> Geometry {
    Geometry::new([8, 128, q], 1)
}
pub fn design(q: u64) -> Design {
    let mut d = crate::suites::pipeline::design(CAP, &geometry(q), 64, Mode::Instant, 1);
    d.top = "midi_audio_pipeline".into();
    d.probes.clear();
    d.includes.push(crate::root().join("fpga/midi/rtl"));
    d.sources.extend(
        [
            "midi_audio_pipeline",
            "midi_data_jobs",
            "midi_store",
            "midi_expression_state",
        ]
        .iter()
        .map(|n| crate::root().join(format!("fpga/midi/rtl/{n}.sv"))),
    );
    for (k, v) in [
        ("MIDI_LOOPS", 2),
        ("MIDI_INPUTS", 2),
        ("MIDI_DESTINATIONS", 2),
        ("MIDI_BUFFERS", 8),
        ("MIDI_CURSORS", 2),
        ("MIDI_SCRATCH_EVENTS", 16384),
        ("MIDI_BLOCK_EVENTS", 8),
        ("MIDI_BLOCKS", 512),
        ("MIDI_MAX_EVENTS", 4096),
        ("MIDI_VIEWS", 32),
        ("MIDI_RETIREMENTS", 32),
    ] {
        d.parameters.insert(k.into(), v.to_string());
    }
    d
}
pub fn compile(q: u64) -> Result<Compiled> {
    design(q).build(&crate::root().join("work/marlin-suite/midi-data-pipeline"))
}
pub struct Driver<'a> {
    pub sim: Sim<'a>,
    pub audio: Audio,
    pub store: Store,
    pub reference: Oracle,
    pub memory: Rc<RefCell<Memory>>,
    pub sink: Rc<RefCell<Sink>>,
    pub address: usize,
}
impl<'a> Driver<'a> {
    pub fn new(compiled: &'a Compiled, q: u32, mixed: bool) -> Result<Self> {
        Self::build(compiled, q, mixed, false)
    }
    pub fn build(compiled: &'a Compiled, q: u32, mixed: bool, expression: bool) -> Result<Self> {
        let mut sim = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
        sim.max_ps = 1_000_000_000_000_000;
        let address = (sim.model.get("midi_arena_bytes") as usize + 271) & !15;
        let memory = Rc::new(RefCell::new(Memory::mixed(address / 4 + 32768)));
        let sink = Rc::new(RefCell::new(Sink::mixed()));
        sim.agent(memory.clone());
        sim.agent(sink.clone());
        sim.agent(crate::protocol::ReadyValid::new(
            "midi_memory_valid",
            "midi_memory_ready",
            &[
                "midi_memory_address",
                "midi_memory_write",
                "midi_memory_write_data",
            ],
        ));
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
        let mut audio = Audio::new(&mut sim, CAP, 1401);
        let mut store = Store::new(
            &mut sim,
            geometry(u64::from(q)),
            Mode::Instant,
            1431,
            "store_",
        )?;
        let reference = Oracle::build(q, mixed, expression);
        let logical = if expression {
            super::logical_expression(q, mixed)
        } else {
            logical(q, mixed)
        };
        let rtl = crate::target::RtlTarget::new(CAP, 16, &[0, 1]).build(&logical)?;
        audio.load_compiled(&mut sim, &rtl);
        store.load(
            &mut sim,
            &serde_json::to_value(reference.audio.snapshot()).unwrap(),
        );
        sim.model
            .set_all(&[("sync_event_ready", 1), ("run_quantum", u64::from(q))]);
        let mut d = Self {
            sim,
            audio,
            store,
            reference,
            memory,
            sink,
            address,
        };
        assert_eq!(d.control(0, [0; 5]), 0);
        if expression {
            assert_eq!(d.control(31, [0; 5]), 0);
            assert_eq!(d.sim.model.get("midi_response_count"), 0x45580100);
            for n in 0..2u64 {
                assert_eq!(
                    d.control(
                        21,
                        [n, (1 << 32) | n, n | (1 << 8) | ((1 << n) << 16), 0, 0]
                    ),
                    0
                );
            }
        }
        Ok(d)
    }
    pub fn control(&mut self, op: u64, args: [u64; 5]) -> u64 {
        self.sim.model.set("midi_command_op", op);
        for (i, a) in args.into_iter().enumerate() {
            self.sim.model.set(&format!("midi_command_arg{i}"), a);
        }
        self.sim
            .model
            .set_all(&[("midi_command_valid", 1), ("midi_response_ready", 0)]);
        self.sim.until(2_000_000, "MIDI control admission", |m| {
            m.get("midi_command_ready") != 0
        });
        self.sim.idle(1);
        self.sim.model.set("midi_command_valid", 0);
        self.sim.until(10_000_000, "MIDI control response", |m| {
            m.get("midi_response_valid") != 0 || m.get("midi_publication_valid") != 0
        });
        if self.sim.model.get("midi_publication_valid") != 0 {
            assert_eq!(self.sim.model.get("run_ready"), 0);
            let slot = if op == 6 { args[1] } else { args[0] } as u32;
            let state = self
                .reference
                .pipeline
                .state()
                .loop_state(super::id(slot))
                .unwrap();
            let spec = rt_fpga_reference::transitions::LoopSpec {
                id: state.state.id,
                kind: rt_fpga_reference::transitions::LoopKind::Primitive,
                length: 0,
                sync_source: (slot == 1).then_some(super::id(0)),
                repeat_sync: false,
                timeline: None,
            };
            let row = crate::loop_image::image(state, &spec, None);
            crate::loop_image::write_words(
                &mut self.audio.program,
                &mut self.sim,
                slot,
                &row,
                0,
                16,
            );
            self.sim.model.set("midi_publication_commit", 1);
            self.sim.idle(1);
            self.sim.model.set("midi_publication_commit", 0);
        }
        self.sim.until(100, "publication response", |m| {
            m.get("midi_response_valid") != 0
        });
        let error = self.sim.model.get("midi_response_error");
        self.sim.model.set("midi_response_ready", 1);
        self.sim.idle(1);
        self.sim.model.set("midi_response_ready", 0);
        error
    }
    pub fn install(&mut self, slot: u16, records: &[(u64, Message)], frames: u64) -> ViewId {
        let view = self.reference.midi.store.writer().unwrap();
        assert_eq!(self.control(8, [0; 5]), 0);
        let handle = self.sim.model.get("midi_response_handle");
        for &(frame, message) in records {
            self.reference
                .midi
                .store
                .append(view, frame, message)
                .unwrap();
            assert_eq!(
                self.control(9, [handle, frame, u64::from(super::packed(message)), 0, 0]),
                0
            );
        }
        self.reference.midi.store.finish(view, frames).unwrap();
        assert_eq!(self.control(10, [handle, frames, 0, 0, 0]), 0);
        self.reference
            .pipeline
            .replace_midi_view(
                &mut self.reference.midi,
                super::id(u32::from(slot)),
                SequenceId(slot),
                Some(view),
            )
            .unwrap();
        assert_eq!(self.control(5, [u64::from(slot), handle, 0, 0, 0]), 0);
        self.reference.midi.store.release(view).unwrap();
        assert_eq!(self.control(13, [handle, 0, 0, 0, 0]), 0);
        view
    }
    pub fn duplicate(&mut self, source: u16, destination: u16) {
        let view = self.reference.midi.view(SequenceId(source)).unwrap();
        self.reference
            .pipeline
            .replace_midi_view(
                &mut self.reference.midi,
                super::id(u32::from(destination)),
                SequenceId(destination),
                view,
            )
            .unwrap();
        assert_eq!(
            self.control(6, [u64::from(source), u64::from(destination), 0, 0, 0]),
            0
        );
    }
    pub fn rejected_input(&mut self, events: &[InputEvent]) {
        let before = self.sim.model.get("sync_frame");
        write_events(&mut self.memory.borrow_mut(), self.address, events);
        self.sim.model.set_all(&[
            ("midi_event_address", self.address as u64),
            ("midi_event_count", events.len() as u64),
        ]);
        assert_eq!(
            self.audio
                .execute_with_limit(&mut self.sim, &[], true, 5_000_000),
            (2, 4)
        );
        assert_eq!(self.sim.model.get("midi_error_code"), 1);
        assert_eq!(self.sim.model.get("sync_frame"), before);
        assert!(self.audio.consumed().is_empty());
    }
    pub fn command(&mut self, slot: u32, op: Operation) {
        self.reference.command(slot, op);
    }
    pub fn quantum(&mut self, events: &[InputEvent]) -> u64 {
        update_plans(&mut self.audio, &mut self.sim, &self.reference);
        write_events(&mut self.memory.borrow_mut(), self.address, events);
        self.sink.borrow_mut().events.clear();
        self.sim.model.set_all(&[
            ("midi_event_address", self.address as u64),
            ("midi_event_count", events.len() as u64),
        ]);
        let expected = self.reference.run(events);
        let start = self.sim.cycles;
        let inputs = vec![vec![1 << 22; self.reference.q as usize]];
        let result = self.audio.execute_with_limit(
            &mut self.sim,
            if self.reference.mixed { &inputs } else { &[] },
            true,
            5_000_000,
        );
        assert_eq!(
            result,
            (2, 0),
            "MIDI error {}",
            self.sim.model.get("midi_error_code")
        );
        let clocks = self.sim.cycles - start;
        assert_eq!(
            self.audio.consumed(),
            expected
                .iter()
                .map(|j| serde_json::to_value(j).unwrap())
                .collect::<Vec<_>>()
        );
        assert_eq!(self.sink.borrow().events, self.reference.emitted);
        assert_eq!(
            self.sim.model.get("sync_frame"),
            self.reference.pipeline.state().frame()
        );
        if self.reference.mixed {
            let (_, samples) = self.audio.read_samples(&mut self.sim);
            let expected: Vec<_> = (0..2)
                .map(|channel| {
                    let playing = self
                        .reference
                        .pipeline
                        .state()
                        .loop_state(super::id(channel))
                        .unwrap()
                        .render
                        .mode
                        == rt_fpga_reference::records::Mode::Playing;
                    vec![if playing { 1 << 22 } else { 0 }; self.reference.q as usize]
                })
                .collect();
            assert_eq!(samples, expected);
        }
        for slot in 0..2 {
            let row = crate::loop_image::read_loop(&mut self.audio.program, &mut self.sim, slot);
            assert_eq!(
                row.get(57, 1) != 0,
                self.reference
                    .pipeline
                    .state()
                    .loop_state(super::id(slot))
                    .unwrap()
                    .state
                    .mode
                    == rt_fpga_reference::transitions::Mode::Recording
            );
        }
        clocks
    }
}

pub(super) fn update_plans(audio: &mut Audio, sim: &mut Sim<'_>, reference: &Oracle) {
    for slot in 0..2 {
        let state = reference
            .pipeline
            .state()
            .loop_state(super::id(slot))
            .unwrap();
        let spec = rt_fpga_reference::transitions::LoopSpec {
            id: state.state.id,
            kind: rt_fpga_reference::transitions::LoopKind::Primitive,
            length: 0,
            sync_source: (slot == 1).then_some(super::id(0)),
            repeat_sync: false,
            timeline: None,
        };
        let row = crate::loop_image::image(state, &spec, None);
        crate::loop_image::write_plans(&mut audio.program, sim, slot, &row);
    }
}
