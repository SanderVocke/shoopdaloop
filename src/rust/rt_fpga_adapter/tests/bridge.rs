use rt_fpga_adapter::{
    bridge::{Bridge, Bus, map},
    looping::MediaCommand,
    port::PortIo,
};
use shoop_wasm_test_support::shoop_test;
use std::collections::{BTreeMap, VecDeque};

/// Register-level stand-in for the bridge: every GO keeps its port busy for a few
/// status polls, and reads return scripted values.
#[derive(Default)]
struct Fake {
    identity: u32,
    capacities: BTreeMap<&'static str, u32>,
    registers: BTreeMap<u32, u32>,
    writes: Vec<(u32, u32)>,
    busy: u32,
    busy_polls: u32,
    stuck: bool,
    tx_space: u32,
    pushed: Vec<u32>,
    rx: VecDeque<u32>,
}

impl Fake {
    fn new() -> Self {
        let mut capacities = BTreeMap::new();
        for (name, value) in [
            ("PROGRAM_DEPTH", 256),
            ("STATE_DEPTH", 256),
            ("BUFFERS", 128),
            ("MAX_FRAMES", 32),
            ("INPUT_PORTS", 2),
            ("OUTPUT_PORTS", 2),
            ("LOOP_DEPTH", 4),
            ("REQUEST_DEPTH", 32),
            ("EVENT_DEPTH", 64),
            ("VIEWS", 32),
            ("SLOTS", 32768),
            ("EXTERNAL_IO", 1),
        ] {
            capacities.insert(name, value);
        }
        Self {
            identity: map::IDENTITY,
            capacities,
            tx_space: 3,
            ..Self::default()
        }
    }
    fn written(&self, offset: u32) -> Option<u32> {
        self.writes
            .iter()
            .rev()
            .find(|(o, _)| *o == offset)
            .map(|(_, v)| *v)
    }
}

impl Bus for Fake {
    fn read(&mut self, offset: u32) -> u32 {
        match offset {
            map::ID => self.identity,
            map::VERSION => map::VERSION_NUMBER,
            map::STATUS => {
                if self.stuck || self.busy_polls > 0 {
                    self.busy_polls = self.busy_polls.saturating_sub(1);
                    self.busy
                } else {
                    self.registers.get(&map::STATUS).copied().unwrap_or(0)
                }
            }
            map::STORE_TX_SPACE => self.tx_space,
            map::STORE_RX_LEVEL => self.rx.len() as u32,
            map::STORE_RX => self
                .rx
                .pop_front()
                .expect("RX read only while data is available"),
            _ if (map::CAPACITY..map::CAPACITY + 4 * map::CAPACITY_WORDS as u32)
                .contains(&offset) =>
            {
                let name = map::CAPACITIES[((offset - map::CAPACITY) / 4) as usize];
                self.capacities.get(name).copied().unwrap_or(0)
            }
            _ => self.registers.get(&offset).copied().unwrap_or(0),
        }
    }
    fn write(&mut self, offset: u32, value: u32) {
        self.writes.push((offset, value));
        let busy = |(lsb, _): (u32, u32)| 1 << lsb;
        match offset {
            map::TP_GO => self.busy = busy(map::STATUS_TP_BUSY),
            map::CFG_GO => self.busy = busy(map::STATUS_CFG_BUSY),
            map::STORE_GO => self.busy = busy(map::STATUS_STORE_BUSY),
            map::RUN_GO => self.busy = busy(map::STATUS_RUN_BUSY),
            map::STORE_TX => self.pushed.push(value),
            _ => return,
        }
        if offset != map::STORE_TX {
            self.busy_polls = 2;
        }
    }
}

#[shoop_test]
fn new_bridge_identifies_the_core_and_reads_its_capacities() {
    let mut bridge = Bridge::new(Fake::new()).unwrap();
    let capacities = bridge.capacities();
    assert_eq!(capacities.instructions, 256);
    assert_eq!(capacities.loop_slots, 4);
    assert_eq!(capacities.chunk_limit, 32768);
    assert!(capacities.external_audio);
    assert_eq!(bridge.media_slots(), 128);
    assert!(bridge.arena_capacity(32769).is_err());

    let mut wrong = Fake::new();
    wrong.identity = 0;
    assert!(Bridge::new(wrong).is_err());
}

#[shoop_test]
fn transition_writes_the_request_waits_and_returns_the_response() {
    let mut fake = Fake::new();
    for i in 0..26 {
        fake.registers.insert(map::TP_RESPONSE + 4 * i, 100 + i);
    }
    fake.registers.insert(map::TP_ERROR, 3);
    let mut bridge = Bridge::new(fake).unwrap();
    let mut data = [0; 20];
    data[19] = 0xabcd;
    let reply = bridge.transition(12, 7, data).unwrap();
    assert_eq!(reply.error, 3);
    assert_eq!(reply.words[0], 100);
    assert_eq!(reply.words[25], 125);
    let fake = bridge.bus();
    assert_eq!(fake.written(map::TP_OPCODE), Some(12));
    assert_eq!(fake.written(map::TP_INDEX), Some(7));
    assert_eq!(fake.written(map::TP_DATA + 4 * 19), Some(0xabcd));
    let go = fake
        .writes
        .iter()
        .position(|(o, _)| *o == map::TP_GO)
        .unwrap();
    assert!(fake.writes[..go].iter().any(|(o, _)| *o == map::TP_DATA));
}

#[shoop_test]
fn configuration_selects_memory_and_direction_and_reports_rejection() {
    let mut fake = Fake::new();
    fake.registers.insert(map::CFG_READ + 8, 9);
    let mut bridge = Bridge::new(fake).unwrap();
    let words = bridge.configure(true, 5, None).unwrap();
    assert_eq!(words[2], 9);
    assert_eq!(bridge.bus().written(map::CFG_CONTROL), Some(0));
    bridge.configure(false, 6, Some([1, 2, 3, 4, 5])).unwrap();
    assert_eq!(bridge.bus().written(map::CFG_CONTROL), Some(0b11));
    assert_eq!(bridge.bus().written(map::CFG_WRITE + 16), Some(5));
    bridge.bus_mut().registers.insert(map::CFG_ERROR, 1);
    assert!(bridge.configure(false, 6, None).is_err());
}

#[shoop_test]
fn media_commands_stream_within_fifo_space_and_drain_reads() {
    let mut fake = Fake::new();
    fake.rx.extend([10, 11, 12]);
    fake.registers.insert(map::STORE_RESULT, 0x100 | 2);
    fake.registers.insert(map::STORE_RESULT_VIEW, 4);
    fake.registers.insert(map::STORE_WRITTEN, 5);
    let mut bridge = Bridge::new(fake).unwrap();
    let command = MediaCommand {
        opcode: 5,
        view: 4,
        start: 1 << 33,
        length: 7,
        count: 5,
    };
    let reply = bridge.store_command(command, &[1, 2, 3, 4, 5]).unwrap();
    assert_eq!(reply.code, 2);
    assert!(reply.result.empty);
    assert_eq!(reply.result.view, 4);
    assert_eq!(reply.result.samples, [10, 11, 12]);
    assert_eq!(reply.written, 5);
    let fake = bridge.bus();
    assert_eq!(fake.pushed, [1, 2, 3, 4, 5]);
    assert_eq!(fake.written(map::STORE_START + 4), Some(2));
    assert_eq!(fake.written(map::STORE_LENGTH), Some(7));
}

#[shoop_test]
fn stuck_ports_time_out_instead_of_waiting_forever() {
    let mut fake = Fake::new();
    fake.stuck = true;
    let mut bridge = Bridge::new(fake).unwrap();
    bridge.poll_limit = 10;
    let error = bridge.transition(0, 0, [0; 20]).unwrap_err();
    assert!(error.contains("timeout"), "{error}");
}
