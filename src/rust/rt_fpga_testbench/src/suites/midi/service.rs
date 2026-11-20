use super::{Memory, command, design, differences, projection};
use crate::{
    Result,
    clock::Edges,
    model::Model,
    random::Random,
    sim::{Agent, Sim},
};
use rt_fpga_reference::midi::{Message, MidiState, StateTransition};
use std::{cell::RefCell, rc::Rc};

struct Sink {
    rng: Random,
    messages: Vec<Message>,
    refuse: bool,
}
impl Agent for Sink {
    fn drive(&mut self, m: &mut Model<'_>, e: Edges) {
        if e.rises(0) {
            m.set("packet_ready", u64::from(self.rng.yes(3)));
            m.set("packet_refused", u64::from(self.refuse));
        }
    }
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if e.rises(0)
            && m.get("reset") == 0
            && m.get("packet_valid") != 0
            && m.get("packet_ready") != 0
            && !self.refuse
        {
            let data = m.get("packet_data");
            let bytes = [data as u8, (data >> 8) as u8, (data >> 16) as u8];
            self.messages
                .push(Message::managed(&bytes[..m.get("packet_length") as usize]).unwrap());
        }
    }
}
pub fn service_conformance() -> Result<()> {
    let compiled = design(
        "midi_state_service",
        &[
            "midi_state_service",
            "midi_state",
            "midi_difference",
            "midi_transition",
        ],
    )?;
    let memory = Rc::new(RefCell::new(Memory::new(4225 * 3 + 133 * 2 + 32)));
    let sink = Rc::new(RefCell::new(Sink {
        rng: Random::new(147),
        messages: Vec::new(),
        refuse: false,
    }));
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
    s.max_ps = 100_000_000_000_000;
    s.agent(memory.clone());
    s.agent(sink.clone());
    s.agent(crate::protocol::ReadyValid::new(
        "packet_valid",
        "packet_ready",
        &["packet_data", "packet_length"],
    ));
    s.model.set("reset", 1);
    s.idle(1);
    s.model.set("reset", 0);
    let mut target = MidiState::default();
    for p in [
        &[0xb0, 0, 4][..],
        &[0xc0, 12],
        &[0xb0, 0, 7],
        &[0x90, 60, 80],
        &[0xb0, 66, 127],
        &[0x80, 60, 0],
        &[0xb0, 1, 43],
    ] {
        target.apply_packet(p, 3).unwrap();
    }
    let mut current = MidiState::default();
    let alternate = MidiState::default();
    const MAP: usize = 4225 * 3;
    const DIR: usize = MAP + 266;
    const HEADER: usize = DIR + 24;
    {
        let mut m = memory.borrow_mut();
        m.words[..4225].copy_from_slice(&projection(&target));
        m.words[4225..8450].copy_from_slice(&projection(&current));
        m.words[8450..12675].copy_from_slice(&projection(&alternate));
        m.words[MAP..DIR].fill(0);
        m.words[HEADER..HEADER + 4].copy_from_slice(&[3, 1, 8, 1]);
        for (i, base) in [0, 8450].into_iter().enumerate() {
            let entry = [
                1,
                0,
                (base * 4) as u32,
                0,
                4225 * 4,
                0,
                ((MAP + i * 133) * 4) as u32,
                0,
                ((HEADER + i * 2) * 4) as u32,
                0,
                if i == 0 { 3 } else { 8 },
                4225,
            ];
            m.words[DIR + i * 12..DIR + (i + 1) * 12].copy_from_slice(&entry);
        }
    }
    for i in 0..2 {
        s.model.set_all(&[
            ("command_op", 2),
            ("command_current", 4225 * 4),
            ("command_target", if i == 0 { 0 } else { 8450 * 4 }),
            ("command_map", ((MAP + i * 133) * 4) as u64),
            ("command_first", 0),
            ("command_count", 4225),
        ]);
        assert_eq!(command(&mut s), 0);
    }
    s.model.set_all(&[
        ("command_directory", (DIR * 4) as u64),
        ("command_pairs", 2),
        ("command_op", 1),
    ]);
    for p in [
        &[0x90, 65, 99][..],
        &[0xb0, 64, 127],
        &[0xb0, 1, 55],
        &[0xc0, 7],
        &[0xb0, 64, 0],
    ] {
        let packet = p
            .iter()
            .enumerate()
            .fold(0, |n, (i, b)| n | u64::from(*b) << (i * 8));
        s.model.set_all(&[
            ("command_packet", packet),
            ("command_length", p.len() as u64),
        ]);
        assert_eq!(command(&mut s), 0);
        current.apply_packet(p, 3).unwrap();
        let m = memory.borrow();
        assert_eq!(&m.words[4225..8450], projection(&current));
        assert_eq!(&m.words[MAP..MAP + 133], differences(&target, &current).0);
        assert_eq!(
            &m.words[MAP + 133..DIR],
            differences(&alternate, &current).0
        );
    }
    let mut oracle = current;
    let mut expected = Vec::new();
    let mut t = StateTransition::restore(1);
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
    s.model.set_all(&[
        ("command_op", 3),
        ("command_target", 0),
        ("command_map", (MAP * 4) as u64),
        ("command_channels", 1),
    ]);
    assert_eq!(command(&mut s), 0);
    assert_eq!(sink.borrow().messages, expected);
    assert_eq!(&memory.borrow().words[4225..8450], projection(&oracle));
    assert_eq!(
        &memory.borrow().words[MAP..MAP + 133],
        differences(&target, &oracle).0
    );
    assert_eq!(
        &memory.borrow().words[MAP + 133..DIR],
        differences(&alternate, &oracle).0
    );
    sink.borrow_mut().refuse = true;
    let before = memory.borrow().words[4225..8450].to_vec();
    s.model.set("command_op", 4);
    assert_eq!(command(&mut s), 5);
    assert_eq!(&memory.borrow().words[4225..8450], before);
    sink.borrow_mut().refuse = false;
    s.model.set("command_op", 4);
    assert_eq!(command(&mut s), 0);
    assert!(
        memory.borrow().words[4225..4353]
            .iter()
            .all(|n| n & 0xffff == 0)
    );
    Ok(())
}
