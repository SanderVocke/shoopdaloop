use super::{Memory, command, projection};
use crate::{
    Result,
    clock::Edges,
    model::{Compiled, Design, Model},
    random::Random,
    sim::{Agent, Sim},
};
use rt_fpga_reference::{
    midi::{InputEvent, Message, MidiState, OutputEvent},
    records::{LoopRecord, Mode},
};
use std::{cell::RefCell, rc::Rc};

pub(super) struct Sink {
    messages: Vec<OutputEvent>,
    random: Random,
    remaining: usize,
}
impl Sink {
    pub(super) fn new() -> Self {
        Self {
            messages: Vec::new(),
            random: Random::new(491),
            remaining: usize::MAX,
        }
    }
    pub(super) fn messages(&self) -> &[OutputEvent] {
        &self.messages
    }
}
impl Agent for Sink {
    fn drive(&mut self, m: &mut Model<'_>, e: Edges) {
        if e.rises(0) {
            m.set("packet_ready", u64::from(self.random.yes(3)));
            m.set("packet_refused", u64::from(self.remaining == 0));
        }
    }
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if e.rises(0)
            && m.get("reset") == 0
            && m.get("packet_valid") != 0
            && m.get("packet_ready") != 0
            && self.remaining != 0
        {
            let packed = m.get("packet_data");
            let data = [packed as u8, (packed >> 8) as u8, (packed >> 16) as u8];
            self.messages.push(OutputEvent {
                destination: m.get("packet_destination") as u16,
                offset: m.get("packet_offset") as u32,
                message: Message::managed(&data[..m.get("packet_length") as usize]).unwrap(),
            });
            self.remaining -= 1;
        }
    }
}
pub fn compile(loops: u64) -> Result<Compiled> {
    let sources = [
        "midi_runtime",
        "midi_store",
        "midi_state_service",
        "midi_state",
        "midi_difference",
        "midi_transition",
    ];
    let mut d = Design::new(
        "midi_runtime",
        sources
            .iter()
            .map(|p| crate::root().join(format!("fpga/midi/rtl/{p}.sv")))
            .collect(),
    );
    d.includes.push(crate::root().join("fpga/midi/rtl"));
    d.parameters(&[
        ("LOOPS", loops),
        ("INPUTS", 1),
        ("DESTINATIONS", 2),
        ("BLOCK_EVENTS", 8),
        ("BLOCKS", 128),
        ("MAX_EVENTS", 128),
        ("VIEWS", 64),
        ("SNAPSHOTS", 64),
        ("RETIREMENTS", 32),
        ("INPUT_EVENTS", 16),
        ("MAINTENANCE", 8),
    ])
    .build(&crate::root().join("work/marlin-suite/midi"))
}
pub fn compile_limits(l: rt_fpga_reference::midi::SessionLimits) -> Result<Compiled> {
    let sources = [
        "midi_runtime",
        "midi_store",
        "midi_state_service",
        "midi_state",
        "midi_difference",
        "midi_transition",
    ];
    let mut d = Design::new(
        "midi_runtime",
        sources
            .iter()
            .map(|p| crate::root().join(format!("fpga/midi/rtl/{p}.sv")))
            .collect(),
    );
    d.includes.push(crate::root().join("fpga/midi/rtl"));
    d.parameters(&[
        ("LOOPS", u64::from(l.loops)),
        ("INPUTS", u64::from(l.inputs)),
        ("DESTINATIONS", u64::from(l.destinations)),
        ("BLOCK_EVENTS", u64::from(l.store.block_events)),
        ("BLOCKS", u64::from(l.store.blocks)),
        ("MAX_EVENTS", u64::from(l.store.max_events)),
        ("VIEWS", u64::from(l.store.views)),
        ("SNAPSHOTS", u64::from(l.store.snapshots)),
        ("RETIREMENTS", u64::from(l.store.retirements)),
        ("INPUT_EVENTS", u64::from(l.input_events_per_quantum)),
        ("MAINTENANCE", u64::from(l.maintenance_steps)),
    ])
    .build(&crate::root().join("work/marlin-suite/midi"))
}
pub struct Driver<'a> {
    pub sim: Sim<'a>,
    memory: Rc<RefCell<Memory>>,
    sink: Rc<RefCell<Sink>>,
    pub scratch: u64,
}
impl<'a> Driver<'a> {
    pub fn new(compiled: &'a Compiled, loops: u64) -> Result<Self> {
        Self::with_inputs(compiled, loops, 1)
    }
    pub fn with_inputs(compiled: &'a Compiled, loops: u64, inputs: u64) -> Result<Self> {
        let mut sim = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
        sim.max_ps = 1_000_000_000_000_000;
        let scratch = sim.model.get("arena_bytes") + 256;
        let memory = Rc::new(RefCell::new(Memory::new(scratch as usize / 4 + 8192)));
        let sink = Rc::new(RefCell::new(Sink::new()));
        sim.agent(memory.clone());
        sim.agent(sink.clone());
        sim.agent(crate::protocol::ReadyValid::new(
            "memory_valid",
            "memory_ready",
            &["memory_address", "memory_write", "memory_write_data"],
        ));
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
        sim.model.set("quantum_frames", 16);
        sim.model.set("reset", 1);
        sim.idle(1);
        sim.model.set("reset", 0);
        let mut d = Self {
            sim,
            memory,
            sink,
            scratch,
        };
        assert_eq!(d.issue(0, [0; 5]), 0);
        for input in 0..inputs {
            assert_eq!(d.issue(16, [input, 128, 0, 0, 0]), 0);
        }
        for slot in 0..loops {
            let input = if inputs == loops { slot } else { 0 };
            assert_eq!(
                d.issue(
                    1,
                    [slot, slot | (1 << 32), input, slot / 16, 1 << (slot % 16)]
                ),
                0
            );
        }
        Ok(d)
    }
    pub fn issue(&mut self, op: u64, args: [u64; 5]) -> u64 {
        self.sim.model.set("command_op", op);
        for (i, arg) in args.into_iter().enumerate() {
            self.sim.model.set(&format!("command_arg{i}"), arg);
        }
        command(&mut self.sim)
    }
    pub fn messages(&mut self) -> Vec<OutputEvent> {
        std::mem::take(&mut self.sink.borrow_mut().messages)
    }
    pub fn set_capacity(&mut self, capacity: usize) {
        self.sink.borrow_mut().remaining = capacity;
    }
    pub fn write(&mut self, address: u64, words: &[u32]) {
        self.memory.borrow_mut().words[address as usize / 4..address as usize / 4 + words.len()]
            .copy_from_slice(words);
    }
    pub fn write_state(&mut self, address: u64, state: &MidiState) {
        self.write(address, &projection(state));
    }
    pub fn verify_memory_state(&self, address: u64, state: &MidiState) {
        assert_eq!(self.read(address, 4225), projection(state));
    }
    pub fn read(&self, address: u64, count: usize) -> Vec<u32> {
        self.memory.borrow().words[address as usize / 4..address as usize / 4 + count].to_vec()
    }
    pub fn verify_state(&mut self, kind: u64, index: u64, state: &MidiState) {
        assert_eq!(self.issue(19, [kind, index, 0, 0, 0]), 0);
        let address = self.sim.model.get("response_state") as usize / 4;
        assert_eq!(
            &self.memory.borrow().words[address..address + 4225],
            projection(state)
        );
    }
    pub fn process(
        &mut self,
        start: u64,
        q: u32,
        plans: &[LoopRecord],
        events: &[InputEvent],
    ) -> u64 {
        for (i, p) in plans.iter().enumerate() {
            let mode = match p.mode {
                Mode::Stopped => 0,
                Mode::Recording => 1,
                Mode::Playing => 2,
                Mode::Invalid => 3,
            };
            assert_eq!(
                self.issue(
                    2,
                    [
                        i as u64,
                        mode,
                        p.position,
                        p.content_length,
                        u64::from(p.begin_capture)
                    ]
                ),
                0
            );
        }
        let mut words = Vec::new();
        for e in events {
            let packet = e
                .message
                .data()
                .iter()
                .enumerate()
                .fold((e.message.data().len() as u32) << 24, |n, (i, b)| {
                    n | u32::from(*b) << (i * 8)
                });
            words.extend([u32::from(e.input), e.offset, packet, 0]);
        }
        self.write(self.scratch, &words);
        self.issue(
            3,
            [start, u64::from(q), self.scratch, events.len() as u64, 0],
        )
    }
}
