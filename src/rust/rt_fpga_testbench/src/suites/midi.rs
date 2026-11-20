use crate::{
    Result,
    clock::Edges,
    model::{Design, Model},
    random::Random,
    sim::{Agent, Sim},
};
use rt_fpga_reference::midi::MidiState;
use std::{cell::RefCell, rc::Rc};
mod service;
pub use service::service_conformance;
mod store;
pub use store::store_conformance;
mod axi;
pub mod runtime;
pub use axi::axi_conformance;

pub(crate) fn projection(state: &MidiState) -> Vec<u32> {
    let mut words = Vec::new();
    for channel in 0..16 {
        let c = state.channel(channel).unwrap();
        for key in 0..128 {
            let n = c.note(key).unwrap();
            words.push(
                u32::from(n.velocity())
                    | (u32::from(n.held()) << 8)
                    | (u32::from(n.sostenuto()) << 9)
                    | (u32::from(n.pressure().unwrap_or(128)) << 16),
            );
        }
        words.extend((0..128).map(|cc| u32::from(c.controller(cc).unwrap_or(128))));
        words.push(u32::from(c.program().unwrap_or(128)));
        words.extend(c.program_bank().map(|v| u32::from(v.unwrap_or(128))));
        words.extend([
            u32::from(c.pitch_bend()),
            u32::from(c.pressure().unwrap_or(128)),
            u32::from(c.reset_controllers_seen()),
            0,
            0,
        ]);
    }
    words.push(u32::from(state.system_reset_seen()));
    words
}

pub(crate) struct Memory {
    pins: [&'static str; 9],
    pub words: Vec<u32>,
    rng: Random,
    pending: Option<(u64, u32, bool)>,
    pub fail: Option<usize>,
    pub transfers: u64,
}
impl Memory {
    pub fn new(words: usize) -> Self {
        Self {
            pins: [
                "memory_ready",
                "memory_response_valid",
                "memory_read_data",
                "memory_response_error",
                "memory_response_ready",
                "memory_valid",
                "memory_address",
                "memory_write",
                "memory_write_data",
            ],
            words: vec![0xdeadbeef; words],
            rng: Random::new(1192016),
            pending: None,
            fail: None,
            transfers: 0,
        }
    }
}
impl Memory {
    fn mixed(words: usize) -> Self {
        let mut memory = Self::new(words);
        memory.pins = [
            "midi_memory_ready",
            "midi_memory_response_valid",
            "midi_memory_read_data",
            "midi_memory_response_error",
            "midi_memory_response_ready",
            "midi_memory_valid",
            "midi_memory_address",
            "midi_memory_write",
            "midi_memory_write_data",
        ];
        memory
    }
}
mod mixed;
pub use mixed::benchmark::benchmark_64;
pub use mixed::compiled_mixed_conformance;
impl Agent for Memory {
    fn drive(&mut self, m: &mut Model<'_>, e: Edges) {
        if !e.rises(0) {
            return;
        }
        m.set(
            self.pins[0],
            u64::from(self.pending.is_none() && self.rng.yes(3)),
        );
        m.set(
            self.pins[1],
            u64::from(self.pending.is_some_and(|p| p.0 == 0)),
        );
        m.set(self.pins[2], self.pending.map_or(0, |p| u64::from(p.1)));
        m.set(self.pins[3], u64::from(self.pending.is_some_and(|p| p.2)));
    }
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if !e.rises(0) {
            return;
        }
        if m.get("reset") != 0 {
            self.pending = None;
            return;
        }
        if m.get(self.pins[1]) != 0 && m.get(self.pins[4]) != 0 {
            self.pending = None;
        } else if let Some(p) = &mut self.pending {
            p.0 = p.0.saturating_sub(1);
        }
        if m.get(self.pins[5]) != 0 && m.get(self.pins[0]) != 0 {
            let address = m.get(self.pins[6]);
            assert_eq!(address % 4, 0);
            let index = address as usize / 4;
            assert!(index < self.words.len(), "DDR word {index} out of range");
            let error = self.fail == Some(index);
            if m.get(self.pins[7]) != 0 && !error {
                self.words[index] = m.get(self.pins[8]) as u32;
            }
            self.transfers += 1;
            self.pending = Some((self.rng.below(4), self.words[index], error));
        }
    }
}
pub(crate) fn design(top: &str, sources: &[&str]) -> Result<crate::model::Compiled> {
    let mut d = Design::new(
        top,
        sources
            .iter()
            .map(|p| crate::root().join(format!("fpga/midi/rtl/{p}.sv")))
            .collect(),
    );
    d.includes.push(crate::root().join("fpga/midi/rtl"));
    d.build(&crate::root().join("work/marlin-suite/midi"))
}
pub(crate) fn command(s: &mut Sim<'_>) -> u64 {
    s.model
        .set_all(&[("command_valid", 1), ("response_ready", 0)]);
    s.until(20_000_000, "command admission", |m| {
        m.get("command_ready") != 0
    });
    s.idle(1);
    s.model.set("command_valid", 0);
    s.until(20_000_000, "MIDI completion", |m| {
        m.get("response_valid") != 0
    });
    let error = s.model.get("response_error");
    s.idle(3);
    assert_eq!(s.model.get("response_error"), error);
    s.model.set("response_ready", 1);
    s.idle(1);
    error
}
pub fn state_conformance() -> Result<()> {
    let compiled = design("midi_state", &["midi_state"])?;
    let memory = Rc::new(RefCell::new(Memory::new(4225)));
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
    s.max_ps = 100_000_000_000_000;
    s.agent(memory.clone());
    s.agent(crate::protocol::ReadyValid::new(
        "memory_valid",
        "memory_ready",
        &["memory_address", "memory_write", "memory_write_data"],
    ));
    s.model.set_all(&[("reset", 1), ("changed_ready", 1)]);
    s.idle(1);
    s.model.set("reset", 0);
    let mut state = MidiState::default();
    s.model.set("command_initialize", 1);
    assert_eq!(command(&mut s), 0);
    assert_eq!(memory.borrow().words, projection(&state));
    s.model.set("command_initialize", 0);
    let mut packets = vec![
        vec![0x90, 60, 91],
        vec![0xb0, 66, 127],
        vec![0x80, 60, 4],
        vec![0xb0, 64, 127],
        vec![0x90, 61, 80],
        vec![0x80, 61, 0],
        vec![0xb0, 66, 0],
        vec![0xb0, 64, 0],
        vec![0xb0, 0, 4],
        vec![0xc0, 12],
        vec![0xb0, 0, 7],
        vec![0xb0, 69, 127],
        vec![0xb0, 121, 0],
        vec![0xff],
    ];
    let mut rng = Random::new(71119);
    for _ in 0..300 {
        let status = 0x80 + rng.below(7) as u8 * 16 + rng.below(16) as u8;
        let mut packet = vec![status, rng.below(128) as u8];
        if status & 0xf0 != 0xc0 && status & 0xf0 != 0xd0 {
            packet.push(rng.below(128) as u8);
        }
        if rt_fpga_reference::midi::Message::managed(&packet).is_ok() {
            packets.push(packet);
        }
    }
    for packet in packets {
        let packed = packet
            .iter()
            .enumerate()
            .fold(0, |n, (i, b)| n | (u64::from(*b) << (i * 8)));
        s.model.set_all(&[
            ("command_packet", packed),
            ("command_length", packet.len() as u64),
        ]);
        assert_eq!(command(&mut s), 0, "{packet:?}");
        state.apply_packet(&packet, 3).unwrap();
        assert_eq!(memory.borrow().words, projection(&state), "{packet:?}");
    }
    let before = memory.borrow().words.clone();
    for (packet, length, error) in [
        (0x0006b0, 3, 2),
        (0x009090, 3, 1),
        (0xf0, 1, 2),
        (0xc0, 3, 1),
    ] {
        s.model
            .set_all(&[("command_packet", packet), ("command_length", length)]);
        assert_eq!(command(&mut s), error);
        assert_eq!(memory.borrow().words, before);
    }
    memory.borrow_mut().fail = Some(60);
    s.model
        .set_all(&[("command_packet", 0x3c90), ("command_length", 3)]);
    assert_eq!(command(&mut s), 7);
    Ok(())
}

fn differences(
    target: &MidiState,
    current: &MidiState,
) -> (Vec<u32>, rt_fpga_reference::midi::Differences) {
    use rt_fpga_reference::midi::{ComparisonId, ContentStore, DifferenceService, StoreLimits};
    let mut store = ContentStore::prepare(StoreLimits {
        block_events: 1,
        blocks: 1,
        max_events: 1,
        views: 1,
        snapshots: 1,
        retirements: 1,
    })
    .unwrap();
    let snapshot = store.snapshot(target).unwrap();
    let mut service = DifferenceService::prepare(1, 1).unwrap();
    let id = ComparisonId {
        target: snapshot,
        destination: 0,
    };
    service.initialize(&store, id, current, 4225).unwrap();
    let diff = *service.get(&store, id).unwrap();
    let mut words = vec![0; 133];
    for cell in 0..4225 {
        if diff.contains(cell) {
            words[cell / 32] |= 1 << (cell % 32);
        }
    }
    (words, diff)
}
pub fn difference_conformance() -> Result<()> {
    let compiled = design("midi_difference", &["midi_difference"])?;
    let memory = Rc::new(RefCell::new(Memory::new(4225 * 2 + 133)));
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
    s.max_ps = 100_000_000_000_000;
    s.agent(memory.clone());
    s.agent(crate::protocol::ReadyValid::new(
        "memory_valid",
        "memory_ready",
        &["memory_address", "memory_write", "memory_write_data"],
    ));
    s.model.set("reset", 1);
    s.idle(1);
    s.model.set("reset", 0);
    let mut target = MidiState::default();
    for p in [
        &[0x90, 60, 99][..],
        &[0xb0, 0, 4],
        &[0xc0, 13],
        &[0xb0, 0, 7],
        &[0xb0, 1, 55],
        &[0xd1, 33],
        &[0xe1, 4, 120],
    ] {
        target.apply_packet(p, 3).unwrap();
    }
    let mut current = MidiState::default();
    {
        let mut m = memory.borrow_mut();
        m.words[..4225].copy_from_slice(&projection(&target));
        m.words[4225..8450].copy_from_slice(&projection(&current));
        m.words[8450..].fill(0);
    }
    s.model.set_all(&[
        ("command_target", 0),
        ("command_current", 4225 * 4),
        ("command_map", 8450 * 4),
        ("command_first", 0),
        ("command_count", 4225),
    ]);
    assert_eq!(command(&mut s), 0);
    assert_eq!(
        &memory.borrow().words[8450..],
        differences(&target, &current).0
    );
    for p in [
        &[0x90, 60, 99][..],
        &[0xb0, 1, 55],
        &[0xc0, 13],
        &[0xb0, 0, 7],
        &[0xb0, 1, 1],
        &[0x80, 60, 0],
        &[0xff],
    ] {
        let old = projection(&current);
        current.apply_packet(p, 3).unwrap();
        let new = projection(&current);
        memory.borrow_mut().words[4225..8450].copy_from_slice(&new);
        for cell in 0..4225 {
            if old[cell] != new[cell] {
                s.model
                    .set_all(&[("command_first", cell as u64), ("command_count", 1)]);
                assert_eq!(command(&mut s), 0);
            }
        }
        assert_eq!(
            &memory.borrow().words[8450..],
            differences(&target, &current).0,
            "{p:?}"
        );
    }
    Ok(())
}

pub fn transition_conformance() -> Result<()> {
    use rt_fpga_reference::midi::{Message, StateTransition};
    let compiled = design("midi_transition", &["midi_transition"])?;
    for scenario in 0..6 {
        let mut target = MidiState::default();
        let mut initial = MidiState::default();
        let packets: Vec<&[u8]> = match scenario {
            0 => vec![&[0x90, 60, 80], &[0x90, 61, 91], &[0xb0, 1, 45]],
            1 => vec![
                &[0xb0, 0, 4],
                &[0xb0, 32, 2],
                &[0xc0, 14],
                &[0xb0, 0, 7],
                &[0xd0, 17],
                &[0xe0, 4, 100],
            ],
            2 => vec![
                &[0x90, 60, 80],
                &[0xb0, 66, 127],
                &[0x80, 60, 0],
                &[0xb0, 64, 127],
                &[0x90, 61, 50],
                &[0x80, 61, 0],
                &[0xa0, 60, 33],
            ],
            3 => vec![
                &[0x91, 60, 80],
                &[0xb1, 69, 127],
                &[0x81, 60, 0],
                &[0xb1, 121, 0],
            ],
            4 => vec![&[0xff]],
            _ => vec![&[0x90, 60, 80], &[0xb0, 64, 127]],
        };
        for p in packets {
            target.apply_packet(p, 3).unwrap();
        }
        for p in [&[0x90, 60, 80][..], &[0x90, 65, 99], &[0xb0, 1, 45]] {
            initial.apply_packet(p, 3).unwrap();
        }
        if scenario == 2 {
            initial.apply_packet(&[0xb0, 64, 127], 3).unwrap();
        }
        let cleanup = scenario == 5;
        let mut expected = Vec::new();
        let mut oracle = initial;
        let mut t = if cleanup {
            StateTransition::cleanup(3)
        } else {
            StateTransition::restore(3)
        };
        for _ in 0..100_000 {
            let diff = differences(&target, &oracle).1;
            let before = oracle;
            t.step(&target, &before, &diff, |message| {
                expected.push(message);
                oracle.apply_packet(message.data(), 3)
            })
            .unwrap();
            if t.complete() {
                break;
            }
        }
        assert!(t.complete());
        let memory = Rc::new(RefCell::new(Memory::new(8583)));
        {
            let mut m = memory.borrow_mut();
            m.words[..4225].copy_from_slice(&projection(&target));
            m.words[4225..8450].copy_from_slice(&projection(&initial));
            m.words[8450..].copy_from_slice(&differences(&target, &initial).0);
        }
        let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
        s.max_ps = 100_000_000_000_000;
        s.agent(memory.clone());
        s.agent(crate::protocol::ReadyValid::new(
            "packet_valid",
            "packet_ready",
            &["packet_data", "packet_length"],
        ));
        s.model.set("reset", 1);
        s.idle(1);
        s.model.set("reset", 0);
        s.model.set_all(&[
            ("command_target", 0),
            ("command_current", 4225 * 4),
            ("command_map", 8450 * 4),
            ("command_channels", 3),
            ("command_cleanup", u64::from(cleanup)),
            ("command_valid", 1),
            ("response_ready", 0),
        ]);
        s.idle(1);
        s.model.set("command_valid", 0);
        let mut observed = Vec::new();
        let mut current = initial;
        for _ in 0..2_000_000 {
            s.until(2_000_000, "transition proposal", |m| {
                m.get("packet_valid") != 0 || m.get("response_valid") != 0
            });
            if s.model.get("response_valid") != 0 {
                break;
            }
            let data = s.model.get("packet_data");
            let len = s.model.get("packet_length") as usize;
            s.idle(4);
            assert_eq!(s.model.get("packet_data"), data);
            let bytes = [data as u8, (data >> 8) as u8, (data >> 16) as u8];
            let message = Message::managed(&bytes[..len]).unwrap();
            observed.push(message);
            current.apply_packet(message.data(), 3).unwrap();
            {
                let mut m = memory.borrow_mut();
                m.words[4225..8450].copy_from_slice(&projection(&current));
                m.words[8450..].copy_from_slice(&differences(&target, &current).0);
            }
            s.model.set("packet_ready", 1);
            s.idle(1);
            s.model.set("packet_ready", 0);
        }
        assert_eq!(s.model.get("response_error"), 0);
        assert_eq!(observed, expected, "scenario {scenario}");
        assert_eq!(current, oracle, "scenario {scenario}");
    }
    Ok(())
}
