#![cfg(feature = "rt-interface")]
#[cfg(all(feature = "sim", not(target_arch = "wasm32")))]
#[path = "support/semantic_transport_axi.rs"]
mod axi;
#[path = "support/semantic_transport_results.rs"]
mod results;
#[path = "support/semantic_transport_sessions.rs"]
mod sessions;
#[path = "support/semantic_transport_snapshots.rs"]
mod snapshots;
use rt_fpga_rtl::host::{
    Contract, Error, Mailbox, RecoveryState, RegisterBus,
    semantic::{
        CommandRecord, Device, GroupHeader, GroupPacket, MAX_PUBLICATION_BUS_OPS, PublicationStep,
    },
    semantic_registers::*,
};
use shoop_wasm_test_support::shoop_test;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

#[derive(Clone, Copy, Debug)]
enum Op {
    Read(u32),
    Write(u32, u32),
}
struct State {
    registers: BTreeMap<u32, u32>,
    ops: Vec<Op>,
    words: [Vec<u32>; 4],
    states: [u32; 4],
    results: [Vec<u32>; 4],
    result_bank: usize,
    result_cursor: usize,
    acknowledgements: usize,
    snapshots: [Option<Vec<u32>>; 2],
    snapshot_bank: usize,
    snapshot_cursor: usize,
    snapshot_reads: usize,
    snapshot_acks: Vec<(usize, u64)>,
    snapshot_refill: Option<(usize, Vec<u32>)>,
    forced_snapshot_receipt: Option<u32>,
    bank: usize,
    admitted: Vec<(usize, Vec<u32>)>,
    fail: Option<(usize, bool)>,
    reset_at: Option<usize>,
    forced_free: Option<u32>,
    forced_receipt: Option<u32>,
    recovery_requests: usize,
    services: Vec<(u32, [u32; 14])>,
    service_pops: usize,
}
#[derive(Clone)]
struct Bus(Rc<RefCell<State>>);
fn contract() -> Contract {
    Contract {
        clock_hz: 100_000_000,
        arena_base: 0x3000_0000,
        arena_bytes: 0x0800_0000,
    }
}
impl Bus {
    fn new() -> Self {
        Self(Rc::new(RefCell::new(State {
            registers: BTreeMap::from([
                (REG_ID, SEMANTIC_DISCOVERY_ID),
                (REG_VERSION, 16),
                (REG_EPOCH, 1),
                (REG_CORE_CLOCK_HZ, 100_000_000),
                (REG_ARENA_BASE, 0x3000_0000),
                (REG_ARENA_BYTES, 0x0800_0000),
                (REG_PROGRAM_DEPTH, 1024),
                (REG_STATE_DEPTH, 512),
                (REG_JOB_DEPTH, 512),
                (REG_BUFFERS, 512),
                (REG_VIEWS, 512),
                (REG_MEDIA_BLOCKS, 31 * 1024),
                (REG_CHUNK_FRAMES, 32),
                (REG_BLOCK_CHUNKS, 32),
                (REG_QUANTUM, 32),
                (REG_SEMANTIC_ID, SEMANTIC_ID),
                (REG_SEMANTIC_CAPABILITIES, SEMANTIC_CAPABILITIES),
                (REG_GROUP_BANKS, 4),
                (REG_COMMANDS_PER_GROUP, 32),
                (REG_SNAPSHOT_LOOP_LIMIT, 128),
                (REG_OBSERVATION_ROW_WORDS, 19),
                (REG_COMMAND_CYCLE_LIMIT, 4096),
                (REG_SNAPSHOT_CYCLE_LIMIT, 8192),
                (REG_QUANTUM_CYCLE_LIMIT, 60000),
                (REG_RECOVERY_ID, 0x5253_5431),
                (REG_RECOVERY_STATUS, 0),
                (REG_STATUS, 1),
                (REG_NEXT_SEQUENCE_LOW, 1),
            ]),
            ops: Vec::new(),
            words: std::array::from_fn(|_| Vec::new()),
            states: [0; 4],
            results: std::array::from_fn(|_| Vec::new()),
            result_bank: 0,
            result_cursor: 0,
            acknowledgements: 0,
            snapshots: [None, None],
            snapshot_bank: 0,
            snapshot_cursor: 0,
            snapshot_reads: 0,
            snapshot_acks: Vec::new(),
            snapshot_refill: None,
            forced_snapshot_receipt: None,
            bank: 0,
            admitted: Vec::new(),
            fail: None,
            reset_at: None,
            forced_free: None,
            forced_receipt: None,
            recovery_requests: 0,
            services: Vec::new(),
            service_pops: 0,
        })))
    }
    fn set(&self, register: u32, value: u32) {
        self.0.borrow_mut().registers.insert(register, value);
    }
    fn open(&self) -> Device<Self> {
        Device::open(self.clone(), contract()).unwrap()
    }
}
impl State {
    fn op(&mut self, op: Op) -> Result<bool, u32> {
        self.ops.push(op);
        if self.reset_at == Some(self.ops.len()) {
            let epoch = self.registers[&REG_EPOCH];
            self.registers.insert(REG_EPOCH, epoch + 1);
        }
        match self.fail {
            Some((at, false)) if at == self.ops.len() => Err(99),
            Some((at, true)) if at == self.ops.len() => Ok(true),
            _ => Ok(false),
        }
    }
    fn reset_runtime(&mut self) {
        let epoch = self.registers[&REG_EPOCH] + 1;
        self.registers.insert(REG_EPOCH, epoch);
        self.registers.insert(REG_RECOVERY_STATUS, 3);
        self.registers.insert(REG_STATUS, 1);
        self.services.clear();
        self.service_pops = 0;
        self.registers.insert(REG_NEXT_SEQUENCE_LOW, 1);
        self.registers.insert(REG_NEXT_SEQUENCE_HIGH, 0);
        self.registers.insert(REG_LAST_ADMITTED_LOW, 0);
        self.registers.insert(REG_LAST_ADMITTED_HIGH, 0);
        self.states = [0; 4];
        self.words.iter_mut().for_each(Vec::clear);
        self.results.iter_mut().for_each(Vec::clear);
        self.result_bank = 0;
        self.result_cursor = 0;
        self.registers.insert(REG_RESULT_RECEIPT, 0);
        self.registers.remove(&REG_RESULT_READY);
        self.snapshots = [None, None];
        self.snapshot_bank = 0;
        self.snapshot_cursor = 0;
        self.snapshot_refill = None;
        self.registers.insert(REG_SNAPSHOT_RECEIPT, 0);
        self.registers.remove(&REG_SNAPSHOT_READY);
    }
}
impl RegisterBus for Bus {
    type Error = u32;
    fn read(&mut self, register: u32) -> Result<u32, u32> {
        let mut s = self.0.borrow_mut();
        let fail = s.op(Op::Read(register))?;
        let value = match register {
            REG_FREE_BANKS => s.forced_free.unwrap_or_else(|| {
                s.states
                    .iter()
                    .enumerate()
                    .fold(0, |m, (i, v)| m | (u32::from(*v == 0) << i))
            }),
            REG_COMMAND_RECEIPT => s.forced_receipt.unwrap_or(0),
            REG_RESULT_READY => s.registers.get(&register).copied().unwrap_or_else(|| {
                (0..4)
                    .filter(|&bank| s.states[bank] >= 2)
                    .min_by_key(|&bank| {
                        u64::from(s.words[bank][4]) | (u64::from(s.words[bank][5]) << 32)
                    })
                    .map_or(0, |bank| if s.states[bank] == 5 { 1 << bank } else { 0 })
            }),
            REG_SNAPSHOT_READY => s.registers.get(&register).copied().unwrap_or_else(|| {
                u32::from(s.snapshots[0].is_some()) | (u32::from(s.snapshots[1].is_some()) << 1)
            }),
            REG_SNAPSHOT_RECEIPT => s
                .forced_snapshot_receipt
                .unwrap_or_else(|| *s.registers.get(&register).unwrap_or(&0)),
            REG_SNAPSHOT_DATA => {
                s.snapshot_reads += 1;
                let bank = s.snapshot_bank;
                let value = s.snapshots[bank]
                    .as_ref()
                    .and_then(|words| words.get(s.snapshot_cursor))
                    .copied();
                if let Some(value) = value {
                    s.snapshot_cursor += 1;
                    s.registers.insert(REG_SNAPSHOT_RECEIPT, 0);
                    value
                } else {
                    s.registers
                        .insert(REG_SNAPSHOT_RECEIPT, RECEIPT_UNAVAILABLE);
                    0
                }
            }
            REG_RESULT_DATA => {
                let bank = s.result_bank;
                let cursor = s.result_cursor;
                if let Some(value) = s.results[bank].get(cursor).copied() {
                    s.result_cursor += 1;
                    value
                } else {
                    s.registers.insert(REG_RESULT_RECEIPT, RECEIPT_UNAVAILABLE);
                    0
                }
            }
            _ => *s.registers.get(&register).unwrap_or(&0),
        };
        if fail { Err(99) } else { Ok(value) }
    }
    fn write(&mut self, register: u32, value: u32) -> Result<(), u32> {
        let mut s = self.0.borrow_mut();
        let fail = s.op(Op::Write(register, value))?;
        let bank = s.bank;
        match register {
            r if (REG_REQUEST_EPOCH..REG_REQUEST_PUSH).contains(&r) => {
                s.registers.insert(register, value);
                s.registers.insert(REG_SUBMISSION_RESULT, 0);
            }
            REG_REQUEST_PUSH => {
                let words = std::array::from_fn(|i| {
                    *s.registers
                        .get(&(REG_REQUEST_EPOCH + 4 * i as u32))
                        .unwrap_or(&0)
                });
                if words[0] != s.registers[&REG_EPOCH] {
                    s.registers.insert(REG_SUBMISSION_RESULT, RECEIPT_EPOCH);
                } else {
                    assert_ne!(value, 0);
                    s.services.push((value, words));
                    s.registers.insert(REG_STATUS, 2);
                    s.registers.insert(REG_RESPONSE_TAG, value);
                    s.registers.insert(REG_RESPONSE_CODE, 0);
                    s.registers.insert(REG_SUBMISSION_RESULT, 0);
                    for i in 0..12 {
                        s.registers.insert(REG_RESPONSE_DATA + 4 * i, 0);
                    }
                }
            }
            REG_RESPONSE_POP => {
                assert_eq!(value, s.registers[&REG_RESPONSE_TAG]);
                assert_eq!(s.registers[&REG_STATUS], 2);
                s.service_pops += 1;
                s.registers.insert(REG_STATUS, 1);
                s.registers.insert(REG_SUBMISSION_RESULT, 0);
            }
            REG_COMMAND_BANK => {
                assert!(value < 4);
                s.bank = value as usize;
            }
            REG_COMMAND_BEGIN => {
                assert_eq!(value, s.registers[&REG_EPOCH]);
                assert!(s.states[bank] <= 1);
                s.words[bank].clear();
                s.states[bank] = 1;
            }
            REG_COMMAND_DATA => {
                assert_eq!(s.states[bank], 1);
                s.words[bank].push(value);
            }
            REG_COMMAND_PUBLISH => {
                assert_eq!(value, s.registers[&REG_EPOCH]);
                assert_eq!(s.states[bank], 1);
                if s.forced_receipt.is_none_or(|code| code == 0) {
                    let words = s.words[bank].clone();
                    assert_eq!(words.len(), 16 + 12 * (words[1] >> 16) as usize);
                    assert_eq!(words[2], value);
                    assert_eq!(words[4], s.registers[&REG_NEXT_SEQUENCE_LOW]);
                    s.admitted.push((bank, words.clone()));
                    s.states[bank] = 2;
                    s.registers.insert(REG_LAST_ADMITTED_LOW, words[4]);
                    s.registers.insert(REG_NEXT_SEQUENCE_LOW, words[4] + 1);
                }
            }
            REG_SNAPSHOT_BANK => {
                s.snapshot_bank = value as usize;
                s.snapshot_cursor = 0;
            }
            REG_SNAPSHOT_REWIND => {
                assert_eq!(value, 1);
                s.snapshot_cursor = 0;
            }
            REG_SNAPSHOT_ACK_LOW | REG_SNAPSHOT_ACK_HIGH | REG_SNAPSHOT_ACK_EPOCH => {
                s.registers.insert(register, value);
            }
            REG_SNAPSHOT_ACK => {
                assert_eq!(value, 1);
                let bank = s.snapshot_bank;
                let sequence = u64::from(s.registers[&REG_SNAPSHOT_ACK_LOW])
                    | (u64::from(s.registers[&REG_SNAPSHOT_ACK_HIGH]) << 32);
                let matches = s.snapshots[bank].as_ref().is_some_and(|words| {
                    u64::from(words[4]) | (u64::from(words[5]) << 32) == sequence
                });
                let receipt = if s.registers[&REG_SNAPSHOT_ACK_EPOCH] != s.registers[&REG_EPOCH] {
                    RECEIPT_EPOCH
                } else if matches {
                    0
                } else {
                    RECEIPT_UNAVAILABLE
                };
                s.registers.insert(REG_SNAPSHOT_RECEIPT, receipt);
                if receipt == 0 {
                    s.snapshots[bank] = None;
                    s.snapshot_acks.push((bank, sequence));
                    if let Some((refill, words)) = s.snapshot_refill.take() {
                        assert!(s.snapshots[refill].is_none());
                        s.snapshots[refill] = Some(words);
                    }
                }
            }
            REG_RESULT_BANK => {
                s.result_bank = value as usize;
                s.result_cursor = 0;
            }
            REG_RESULT_REWIND => {
                assert_eq!(value, 1);
                s.result_cursor = 0;
            }
            REG_RESULT_ACK_LOW | REG_RESULT_ACK_HIGH | REG_RESULT_ACK_EPOCH => {
                s.registers.insert(register, value);
            }
            REG_RESULT_ACK => {
                assert_eq!(value, 1);
                let bank = s.result_bank;
                let sequence = u64::from(s.registers[&REG_RESULT_ACK_LOW])
                    | (u64::from(s.registers[&REG_RESULT_ACK_HIGH]) << 32);
                let original = &s.words[bank];
                if s.registers[&REG_RESULT_ACK_EPOCH] != s.registers[&REG_EPOCH] {
                    s.registers.insert(REG_RESULT_RECEIPT, RECEIPT_EPOCH);
                } else {
                    assert_eq!(s.states[bank], 5);
                    assert_eq!(
                        sequence,
                        u64::from(original[4]) | (u64::from(original[5]) << 32)
                    );
                    s.acknowledgements += 1;
                    s.states[bank] = 0;
                    s.results[bank].fill(0xdead_beef);
                }
            }
            REG_RECOVERY_REQUEST => {
                assert_eq!(value, s.registers[&REG_EPOCH]);
                s.recovery_requests += 1;
                s.reset_runtime();
            }
            _ => panic!("unexpected transport write {register:x}"),
        }
        if fail { Err(99) } else { Ok(()) }
    }
}
fn packet(sequence: u64, count: usize) -> GroupPacket {
    let records = [CommandRecord::gain(None, f32::NAN); 32];
    GroupPacket::new(
        GroupHeader {
            epoch: 1,
            sequence,
            graph: 0xfedc_ba98_7654_3210,
            topology: 0x1234_5678_9abc_def0,
            deadline: Some(u64::MAX),
        },
        &records[..count],
    )
    .unwrap()
}
fn publish(device: &mut Device<Bus>, bus: &Bus) -> usize {
    for _ in 0..1000 {
        let before = bus.0.borrow().ops.len();
        let step = device.publication_step().unwrap();
        assert!(bus.0.borrow().ops.len() - before <= MAX_PUBLICATION_BUS_OPS);
        if let PublicationStep::Published { bank, .. } = step {
            return bank;
        }
    }
    panic!("bounded publication did not finish")
}

#[shoop_test]
fn exact_attachment_refuses_every_profile_mismatch_without_writes_or_legacy_fallback() {
    let registers = Bus::new().0.borrow().registers.clone();
    for (&register, &value) in &registers {
        let bus = Bus::new();
        bus.set(register, value ^ 1);
        assert!(
            Device::open(bus.clone(), contract()).is_err(),
            "register {register:x}"
        );
        assert!(
            bus.0
                .borrow()
                .ops
                .iter()
                .all(|op| matches!(op, Op::Read(_)))
        );
    }
    for version in [15, 17] {
        let bus = Bus::new();
        bus.set(REG_VERSION, version);
        assert!(matches!(
            Device::open(bus, contract()),
            Err(Error::Incompatible)
        ));
    }
    let bus = Bus::new();
    assert!(matches!(
        Mailbox::open(bus.clone(), contract()),
        Err(Error::Incompatible)
    ));
    bus.set(REG_VERSION, 15);
    assert!(Mailbox::open(bus, contract()).is_ok());
    for bit in 0..7 {
        let bus = Bus::new();
        bus.set(
            REG_SEMANTIC_CAPABILITIES,
            SEMANTIC_CAPABILITIES & !(1 << bit),
        );
        assert!(matches!(
            Device::open(bus, contract()),
            Err(Error::Incompatible)
        ));
    }
}

#[shoop_test]
fn attachment_epoch_crossings_cannot_adopt_a_new_runtime_mid_discovery() {
    let bus = Bus::new();
    bus.open();
    let count = bus.0.borrow().ops.len();
    for prefix in 2..=count {
        let bus = Bus::new();
        bus.0.borrow_mut().reset_at = Some(prefix);
        assert!(
            Device::open(bus.clone(), contract()).is_err(),
            "prefix {prefix}"
        );
        assert!(
            bus.0
                .borrow()
                .ops
                .iter()
                .all(|op| matches!(op, Op::Read(_)))
        );
    }
}

#[shoop_test]
fn bounded_publication_preserves_complete_packets_order_and_all_four_reservations() {
    let bus = Bus::new();
    let mut device = bus.open();
    for count in [1, 16, 32, 2] {
        let sequence = device.next_sequence();
        let group = packet(sequence, count);
        let expected = group.words().to_vec();
        device.queue(group).unwrap();
        let bank = publish(&mut device, &bus);
        assert_eq!(device.published(bank).unwrap().sequence(), sequence);
        assert_eq!(bus.0.borrow().admitted.last().unwrap(), &(bank, expected));
    }
    let before = bus.0.borrow().ops.len();
    assert_eq!(device.queue(packet(5, 1)), Err(Error::Full));
    assert_eq!(device.publication_step(), Ok(PublicationStep::Idle));
    assert_eq!(bus.0.borrow().ops.len(), before);
    assert_eq!(bus.0.borrow().admitted.len(), 4);
    assert_eq!(device.next_sequence(), 5);
    let s = bus.0.borrow();
    assert_eq!(
        s.ops
            .iter()
            .filter(|op| matches!(op, Op::Write(REG_COMMAND_PUBLISH, 1)))
            .count(),
        4
    );
    assert!(
        s.ops
            .iter()
            .any(|op| matches!(op, Op::Read(REG_COMMAND_RECEIPT)))
    );
}

#[shoop_test]
fn every_publication_bus_error_prefix_is_quarantined_without_replay_or_fake_admission() {
    let reference = Bus::new();
    let mut device = reference.open();
    let start = reference.0.borrow().ops.len();
    device.queue(packet(1, 1)).unwrap();
    publish(&mut device, &reference);
    let count = reference.0.borrow().ops.len() - start;
    for after in [false, true] {
        for prefix in 1..=count {
            let bus = Bus::new();
            let mut device = bus.open();
            let start = bus.0.borrow().ops.len();
            bus.0.borrow_mut().fail = Some((start + prefix, after));
            device.queue(packet(1, 1)).unwrap();
            loop {
                match device.publication_step() {
                    Err(Error::Bus(99)) => break,
                    Ok(PublicationStep::Progress) => {}
                    other => panic!("prefix {prefix} / {after}: {other:?}"),
                }
            }
            let before = bus.0.borrow().ops.len();
            assert_eq!(device.publication_step(), Err(Error::Fault));
            assert_eq!(device.queue(packet(1, 1)), Err(Error::Fault));
            assert_eq!(bus.0.borrow().ops.len(), before);
            assert!(bus.0.borrow().admitted.len() <= 1);
            assert_eq!(device.next_sequence(), 1);
            assert!(device.published(0).is_none());
        }
    }
}

#[shoop_test]
fn space_polling_and_expected_refusals_never_publish_partial_groups() {
    let bus = Bus::new();
    let mut device = bus.open();
    device.queue(packet(1, 1)).unwrap();
    bus.0.borrow_mut().forced_free = Some(0);
    for _ in 0..10 {
        assert!(matches!(
            device.publication_step(),
            Ok(PublicationStep::Progress | PublicationStep::Waiting)
        ));
    }
    assert!(
        bus.0
            .borrow()
            .ops
            .iter()
            .all(|op| matches!(op, Op::Read(_)))
    );
    bus.0.borrow_mut().forced_free = None;
    publish(&mut device, &bus);
    for code in 1..=9 {
        let bus = Bus::new();
        let mut device = bus.open();
        device.queue(packet(1, 1)).unwrap();
        bus.0.borrow_mut().forced_receipt = Some(code);
        let error = loop {
            if let Err(error) = device.publication_step() {
                break error;
            }
        };
        assert_eq!(
            error,
            match code {
                2 => Error::EpochChanged,
                3 => Error::Fault,
                9 => Error::UnexpectedResponse,
                _ => Error::Rejected(code),
            }
        );
        assert!(bus.0.borrow().admitted.is_empty());
        let ops = bus.0.borrow().ops.len();
        assert_eq!(device.publication_step(), Err(Error::Fault));
        assert_eq!(bus.0.borrow().ops.len(), ops);
    }
}

#[shoop_test]
fn typed_recovery_reuses_the_shared_owner_without_exposing_abi16_as_legacy() {
    let bus = Bus::new();
    let device = bus.open();
    let mut recovery = device.recovery();
    let start = bus.0.borrow().ops.len();
    bus.0.borrow_mut().fail = Some((start + 4, true));
    assert_eq!(recovery.begin(), Err(Error::Bus(99)));
    assert_eq!(recovery.begin(), Err(Error::InvalidRequest));
    assert_eq!(recovery.poll(), Ok(RecoveryState::Ready(2)));
    assert_eq!(bus.0.borrow().recovery_requests, 1);
    let fresh = recovery.reopen().unwrap();
    assert_eq!(fresh.epoch(), 2);
    assert_eq!(fresh.next_sequence(), 1);
    assert!(matches!(recovery.reopen(), Err(Error::Full)));
    assert_eq!(recovery.begin(), Err(Error::InvalidRequest));
    assert!(matches!(
        Mailbox::open(bus, contract()),
        Err(Error::Incompatible)
    ));
}
