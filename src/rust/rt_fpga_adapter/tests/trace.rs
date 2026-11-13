use rt_fpga_adapter::bridge::{
    Bus, map,
    trace::{self, Access, Mismatch, RecordingBus},
};
use shoop_wasm_test_support::{
    heap::{TrackingAllocator, assert_no_heap},
    shoop_test,
};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

#[derive(Default)]
struct Register {
    value: u32,
    accesses: usize,
}
impl Bus for Register {
    fn read(&mut self, _: u32) -> u32 {
        self.accesses += 1;
        self.value
    }
    fn write(&mut self, _: u32, value: u32) {
        self.accesses += 1;
        self.value = value;
    }
}

#[shoop_test]
fn capture_round_trips_uart_noise_and_replays_reads_and_writes() {
    let mut bus = RecordingBus::new(Register::default(), 3);
    bus.write(map::CONTROL, 0x1234_5678);
    assert_eq!(bus.read(map::STATUS), 0x1234_5678);
    bus.write(map::CONTROL, 0);
    let mut log = String::from("SHOOP_FPGA_BRIDGE_READY\r\n");
    bus.dump(&mut log).unwrap();
    log.push_str("SHOOP_HOST_STOPPED\r\n");
    let records = trace::parse(&log).unwrap();
    assert_eq!(records, bus.records());
    let mut target = Register::default();
    trace::replay(&mut target, &records).unwrap();
    assert_eq!(target.accesses, 3);
    assert_eq!(target.value, 0);
}

#[shoop_test]
fn overflow_preserves_the_prefix_and_does_not_interrupt_the_bus() {
    assert_eq!(core::mem::size_of::<Access>(), 12);
    let mut bus = RecordingBus::new(Register::default(), 1);
    let value = assert_no_heap(|| {
        bus.write(map::CONTROL, 11);
        bus.write(map::CONTROL, 12);
        bus.read(map::STATUS)
    });
    assert_eq!(value, 12);
    assert_eq!(bus.inner().accesses, 3);
    assert_eq!(
        bus.records(),
        &[Access::Write {
            offset: map::CONTROL,
            value: 11
        }]
    );
    assert_eq!(bus.dropped(), 2);
    let capture = assert_no_heap(|| bus.take_capture());
    drop(bus);
    assert_eq!(capture.records().len(), 1);
    assert_eq!(capture.dropped(), 2);
    let mut log = String::new();
    capture.dump(&mut log).unwrap();
    assert!(trace::parse(&log).unwrap_err().contains("overflowed"));
    let mut zero = RecordingBus::new(Register::default(), 0);
    zero.read(map::ID);
    assert!(zero.records().is_empty());
    assert_eq!(zero.dropped(), 1);
}

#[shoop_test]
fn replay_stops_at_the_first_difference_including_status_polls() {
    let accesses = [
        Access::Write {
            offset: map::CONTROL,
            value: 1,
        },
        Access::Read {
            offset: map::STATUS,
            value: 2,
        },
        Access::Write {
            offset: map::CONTROL,
            value: 3,
        },
    ];
    let mut bus = Register::default();
    assert_eq!(
        trace::replay(&mut bus, &accesses),
        Err(Mismatch {
            index: 1,
            offset: map::STATUS,
            expected: 2,
            actual: 1,
        })
    );
    assert_eq!(bus.accesses, 2);
    assert_eq!(bus.value, 1);
}

#[shoop_test]
fn incomplete_corrupt_and_ambiguous_logs_are_rejected() {
    let valid = "SHOOP_FPGA_TRACE_BEGIN 1 1 1 0\nSHOOP_FPGA_BUS 0 R 00000000 53484f50\nSHOOP_FPGA_TRACE_END 1\n";
    trace::parse(valid).unwrap();
    for invalid in [
        String::new(),
        valid.replace("SHOOP_FPGA_TRACE_BEGIN", "UNRELATED"),
        valid.replace("BEGIN 1 1", "BEGIN 2 1"),
        valid.replace("BEGIN 1 1", "BEGIN 1 2"),
        valid.replace("BEGIN 1 1 1 0", "BEGIN 1 1 2 0"),
        valid.replace("END 1", "END 2"),
        valid.replace("SHOOP_FPGA_TRACE_END 1\n", ""),
        valid.replace("BUS 0", "BUS 1"),
        valid.replace(" R ", " X "),
        valid.replace("00000000", "00000001"),
        valid.replace("00000000", "00001000"),
        valid.replace("53484f50", "not-hex"),
        format!("{valid}{valid}"),
        format!("{valid}SHOOP_FPGA_TRACE_END 1\n"),
        format!("{valid}SHOOP_FPGA_BUS 1 R 00000000 53484f50\n"),
        "SHOOP_FPGA_TRACE_BEGIN 1 1 0 0\nSHOOP_FPGA_TRACE_END 0\n".into(),
    ] {
        assert!(trace::parse(&invalid).is_err(), "accepted {invalid:?}");
    }
}
