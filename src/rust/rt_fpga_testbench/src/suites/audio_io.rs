//! Independent frame scoreboard and a RAM-copy consumer on asynchronous clocks.
use crate::{
    Result,
    bits::Bits,
    clock::Edges,
    model::{Design, Model, Probe},
    sim::{Agent, Sim},
};
use std::{cell::RefCell, rc::Rc};
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub q: u64,
    pub p: u64,
    pub banks: u64,
    pub channels: u64,
    pub period_ps: u64,
    pub phase_ps: u64,
}
pub const CONFIGS: &[Config] = &[
    Config {
        q: 1,
        p: 1,
        banks: 2,
        channels: 2,
        period_ps: 10_000,
        phase_ps: 3_000,
    },
    Config {
        q: 3,
        p: 1,
        banks: 3,
        channels: 2,
        period_ps: 13_000,
        phase_ps: 0,
    },
    Config {
        q: 3,
        p: 3,
        banks: 4,
        channels: 2,
        period_ps: 13_000,
        phase_ps: 3_000,
    },
    Config {
        q: 16,
        p: 16,
        banks: 3,
        channels: 8,
        period_ps: 13_000,
        phase_ps: 0,
    },
];
fn sample(channel: u64, frame: u64) -> u32 {
    match frame % 7 {
        0 => 0x80000000 | channel as u32,
        1 => 0x7fffffff - channel as u32,
        _ => {
            ((channel + 1) * 100000 + frame * 37 + 1) as u32
                ^ if frame % 2 != 0 { 0x80000000 } else { 0 }
        }
    }
}
fn packed(c: Config, frame: u64) -> Bits {
    Bits((0..c.channels).map(|ch| sample(ch, frame)).collect())
}
pub fn design(c: Config) -> Design {
    let root = crate::root();
    Design::new("audio_io",["audio_lane_ram","audio_io"].iter().map(|n| root.join(format!("fpga/audio_io/rtl/{n}.sv"))).collect()).parameters(&[("Q",c.q),("P",c.p),("INPUTS",c.channels),("OUTPUTS",c.channels),("RX_BANKS",c.banks),("TX_BANKS",c.banks),("WATCHDOG_CYCLES",100000)]).probes("process_bank produce_bank rx_request_sync rx_ack tx_request tx_ack_sync selected_valid selected_bank tx_request_sync tx_ack capture_write capture_bank tx_request_meta".split_whitespace().map(Probe::observe).collect())
}
struct Ownership;
impl Agent for Ownership {
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if !e.rises(0) || m.get("reset") != 0 || m.get("session_reset") != 0 {
            return;
        }
        if m.get("busy") != 0 {
            let rx = m.get("process_bank");
            let tx = m.get("produce_bank");
            assert_ne!(((m.get("rx_request_sync") ^ m.get("rx_ack")) >> rx) & 1, 0);
            assert_eq!(((m.get("tx_request") ^ m.get("tx_ack_sync")) >> tx) & 1, 0);
            if m.get("output_write") != 0 && m.get("selected_valid") != 0 {
                assert_ne!(
                    tx,
                    m.get("selected_bank"),
                    "processing overwrote playing bank"
                );
            }
        }
        if m.get("selected_valid") != 0 {
            assert_ne!(
                ((m.get("tx_request_sync") ^ m.get("tx_ack")) >> m.get("selected_bank")) & 1,
                0
            );
        }
        if m.get("capture_write") != 0 && m.get("busy") != 0 {
            assert_ne!(
                m.get("capture_bank"),
                m.get("process_bank"),
                "capture overwrote processing bank"
            );
        }
    }
}
struct Consumer {
    c: Config,
    enabled: bool,
    block: u64,
    phase: u64,
    channel: u64,
    frame: u64,
    value: u64,
    late: u64,
    gate: bool,
    written: bool,
}
impl Consumer {
    fn start(&mut self, late: u64, gate: bool) {
        self.enabled = true;
        self.block = 0;
        self.phase = 0;
        self.channel = 0;
        self.frame = 0;
        self.late = late;
        self.gate = gate;
        self.written = false;
    }
}
impl Agent for Consumer {
    fn drive(&mut self, m: &mut Model<'_>, e: Edges) {
        if !e.rises(0) || !self.enabled {
            return;
        }
        m.set_all(&[
            ("process_ready", 0),
            ("input_read", 0),
            ("output_write", 0),
            ("process_done", 0),
        ]);
        if m.get("reset") != 0 || m.get("session_reset") != 0 {
            return;
        }
        match self.phase {
            0 => m.set("process_ready", m.get("process_valid")),
            1 => m.set_all(&[
                ("input_read", 1),
                ("input_address", self.channel * self.c.q + self.frame),
            ]),
            2 => m.set_all(&[
                ("output_write", 1),
                ("output_address", self.channel * self.c.q + self.frame),
                ("output_data", self.value),
            ]),
            3 => {
                if self.gate
                    && (self.block != 0
                        || self.late == 0
                        || m.get("session_frame")
                            >= self.c.q + self.c.p + 1 + (self.late - 1) * self.c.q)
                {
                    m.set("process_done", 1);
                }
            }
            _ => unreachable!(),
        }
    }
    fn observe(&mut self, m: &Model<'_>, e: Edges) {
        if !e.rises(0) || !self.enabled || m.get("reset") != 0 || m.get("session_reset") != 0 {
            return;
        }
        match self.phase {
            0 => {
                if m.get("process_ready") != 0 {
                    assert_eq!(m.get("process_sequence"), self.block);
                    self.phase = 1;
                }
            }
            1 => {
                self.value = m.get("input_data");
                assert_eq!(
                    self.value,
                    u64::from(sample(self.channel, self.block * self.c.q + self.frame))
                );
                self.phase = 2;
            }
            2 => {
                self.frame += 1;
                if self.frame == self.c.q {
                    self.frame = 0;
                    self.channel += 1;
                }
                if self.channel == self.c.channels {
                    self.channel = 0;
                    self.phase = 3;
                    self.written = true;
                } else {
                    self.phase = 1;
                }
            }
            3 => {
                if m.get("process_done") != 0 {
                    self.phase = 0;
                    self.block += 1;
                }
            }
            _ => unreachable!(),
        }
    }
}
struct Bench {
    c: Config,
    consumer: Rc<RefCell<Consumer>>,
}
impl Bench {
    fn new(c: Config, s: &mut Sim<'_>) -> Self {
        let consumer = Rc::new(RefCell::new(Consumer {
            c,
            enabled: false,
            block: 0,
            phase: 0,
            channel: 0,
            frame: 0,
            value: 0,
            late: 0,
            gate: true,
            written: false,
        }));
        s.agent(consumer.clone());
        s.agent(Ownership);
        Self { c, consumer }
    }
    fn ns(&self, s: &mut Sim<'_>, ns: u64) {
        let until = s.ps() + ns * 1000;
        while s.ps() < until {
            s.step();
        }
    }
    fn initialize(&self, s: &mut Sim<'_>) {
        s.model.set_all(&[("epoch", 7), ("reset", 1)]);
        self.ns(s, 100);
        s.model.set("reset", 0);
        self.ns(s, 100);
        s.model.set("enable", 1);
        self.ns(s, 100);
    }
    fn restart(&self, s: &mut Sim<'_>, reset_ns: u64) {
        self.consumer.borrow_mut().enabled = false;
        s.model.set_all(&[
            ("enable", 0),
            ("session_reset", 1),
            ("process_ready", 0),
            ("process_done", 0),
            ("input_read", 0),
            ("output_write", 0),
        ]);
        self.ns(s, reset_ns);
        s.model
            .set_all(&[("session_reset", 0), ("epoch", s.model.get("epoch") + 1)]);
        self.ns(s, 100);
        s.model.set("enable", 1);
        self.ns(s, 100);
    }
    fn frames(&self, s: &mut Sim<'_>, count: u64, missed: &[u64], check: bool) {
        let clock = crate::drivers::frames::Frames {
            ticks: 100u64.max(self.c.channels * 8),
            period_ps: self.c.period_ps,
        };
        for frame in 0..count {
            let (_, output) = clock.pulse(s, Some(&packed(self.c, frame)));
            if check {
                let latency = self.c.q + self.c.p;
                let expected =
                    if frame < latency || missed.contains(&((frame - latency) / self.c.q)) {
                        Bits::zero(self.c.channels as usize * 32)
                    } else {
                        packed(self.c, frame - latency)
                    };
                assert_eq!(
                    output.resized(self.c.channels as usize * 32),
                    expected,
                    "frame {frame}, {:?}",
                    self.c
                );
            }
        }
    }
    fn nominal_and_recoverable_miss(&self, s: &mut Sim<'_>) {
        let count = 9 * self.c.q + self.c.p;
        self.consumer.borrow_mut().start(0, true);
        self.frames(s, count, &[], true);
        assert_eq!(
            s.model.get("recovery_requested") | s.model.get("missed_deadlines"),
            0
        );
        self.consumer.borrow_mut().enabled = false;
        if self.c.q > 1 && self.c.banks >= 3 {
            self.restart(s, 100);
            self.consumer.borrow_mut().start(1, true);
            self.frames(s, count, &[0], true);
            assert_eq!(s.model.get("recovery_requested"), 0);
            assert_eq!(s.model.get("missed_deadlines"), 1);
            assert_eq!(s.model.get("discarded_quanta"), 1);
        }
    }
    fn input_exhaustion_and_rearm(&self, s: &mut Sim<'_>) {
        self.frames(s, (self.c.banks + 1) * self.c.q, &[], false);
        assert_ne!(s.model.get("recovery_requested"), 0);
        assert_eq!(s.model.get("recovery_reason"), 1);
        assert_eq!(s.model.get("input_losses"), 1);
        let before = s.model.get("audio_frame");
        s.model.set_all(&[("session_reset", 1), ("enable", 0)]);
        self.frames(s, self.c.q, &[], false);
        s.model.set_all(&[("session_reset", 0), ("epoch", 11)]);
        self.ns(s, 100);
        s.model.set("enable", 1);
        self.ns(s, 100);
        self.consumer.borrow_mut().start(0, true);
        self.frames(s, 6 * self.c.q + self.c.p, &[], true);
        assert_eq!(s.model.get("recovery_requested"), 0);
        assert!(s.model.get("audio_frame") > before);
    }
    fn reset_in_flight_ownership(&self, s: &mut Sim<'_>) {
        for placement in [
            1,
            self.c.q,
            self.c.q + 1,
            self.c.q + self.c.p,
            2 * self.c.q + self.c.p,
        ]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
        {
            self.consumer.borrow_mut().start(0, true);
            self.frames(s, placement, &[], false);
            self.restart(s, 200);
            self.consumer.borrow_mut().start(0, true);
            self.frames(s, 4 * self.c.q + self.c.p, &[], true);
            assert_eq!(s.model.get("recovery_requested"), 0);
            self.restart(s, 200);
        }
    }
    fn accept(&self, s: &mut Sim<'_>) {
        s.until(10000, "copy-consumer admission", |m| {
            m.get("process_valid") != 0
        });
        s.model.set("process_ready", 1);
        s.idle(1);
        s.model.set("process_ready", 0);
        assert_ne!(s.model.get("busy"), 0);
    }
    fn watchdog_and_core_fault(&self, s: &mut Sim<'_>) {
        self.frames(s, self.c.q, &[], false);
        self.accept(s);
        s.until(110000, "watchdog recovery", |m| {
            m.get("recovery_requested") != 0
        });
        assert_eq!(s.model.get("recovery_reason"), 2);
        self.restart(s, 200);
        self.frames(s, self.c.q, &[], false);
        self.accept(s);
        s.model.set("process_fault", 1);
        s.idle(1);
        assert_ne!(s.model.get("recovery_requested"), 0);
        assert_eq!(s.model.get("recovery_reason"), 3);
        assert_eq!(s.model.get("failure_sequence"), 0);
    }
    fn publication_cutoff(&self, s: &mut Sim<'_>) {
        if self.c.banks < 3 {
            return;
        }
        for placement in ["before", "at", "after"] {
            self.consumer.borrow_mut().start(0, false);
            self.frames(s, self.c.q + self.c.p, &[], false);
            s.until(10_000, "copied output awaiting completion", |_| {
                self.consumer.borrow().written
            });
            if placement == "after" {
                s.edge(1, false);
                s.model.set("frame_prepare", 1);
                s.edge(1, true);
                s.model.set("frame_prepare", 0);
                self.consumer.borrow_mut().gate = true;
            } else {
                self.consumer.borrow_mut().gate = true;
                let signal = if placement == "at" {
                    "tx_request_meta"
                } else {
                    "tx_request_sync"
                };
                let mut crossed = false;
                for _ in 0..100 {
                    s.edge(1, true);
                    if s.model.get(signal) & 1 != 0 {
                        crossed = true;
                        break;
                    }
                }
                assert!(crossed);
                s.edge(1, false);
                s.model.set("frame_prepare", 1);
                s.edge(1, true);
                s.model.set("frame_prepare", 0);
            }
            self.ns(s, 4 * self.c.period_ps / 1000);
            s.edge(1, false);
            s.model.set("frame_tick", 1);
            s.model
                .set_bits("rx_samples", &packed(self.c, self.c.q + self.c.p));
            s.edge(1, true);
            let expected = if placement == "before" {
                packed(self.c, 0)
            } else {
                Bits::zero(self.c.channels as usize * 32)
            };
            assert_eq!(
                s.model
                    .bits("tx_samples")
                    .resized(self.c.channels as usize * 32),
                expected,
                "publication {placement}"
            );
            assert_eq!(
                s.model.get("missed_deadlines"),
                u64::from(placement != "before")
            );
            s.model.set("frame_tick", 0);
            self.restart(s, 200);
        }
    }
    fn consecutive_misses_and_catchup(&self, s: &mut Sim<'_>) {
        if self.c.q == 1 || self.c.banks < 3 || (self.c.p == self.c.q && self.c.banks < 4) {
            return;
        }
        self.consumer.borrow_mut().start(2, true);
        self.frames(s, 11 * self.c.q + self.c.p, &[0, 1], true);
        assert_eq!(s.model.get("recovery_requested"), 0);
        assert_eq!(s.model.get("missed_deadlines"), 2);
        assert_eq!(s.model.get("discarded_quanta"), 2);
    }
}
pub fn run(c: Config) -> Result<()> {
    let compiled = design(c).build(&crate::root().join("work/marlin-suite/audio-io"))?;
    for (name, test) in [
        (
            "nominal_and_recoverable_miss",
            Bench::nominal_and_recoverable_miss as fn(&Bench, &mut Sim<'_>),
        ),
        (
            "input_exhaustion_and_rearm",
            Bench::input_exhaustion_and_rearm,
        ),
        (
            "reset_in_flight_ownership",
            Bench::reset_in_flight_ownership,
        ),
        ("watchdog_and_core_fault", Bench::watchdog_and_core_fault),
        ("publication_cutoff", Bench::publication_cutoff),
        (
            "consecutive_misses_and_catchup",
            Bench::consecutive_misses_and_catchup,
        ),
    ] {
        println!("audio-io/{c:?}/{name}");
        let mut s = Sim::new(
            compiled.session(None)?,
            &[("clk", 10_000, 0), ("audio_clk", c.period_ps, c.phase_ps)],
        );
        let d = Bench::new(c, &mut s);
        d.initialize(&mut s);
        test(&d, &mut s);
    }
    Ok(())
}
