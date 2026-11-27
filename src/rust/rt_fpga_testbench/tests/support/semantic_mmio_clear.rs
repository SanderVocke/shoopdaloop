use super::configuration::{request_writes, response_ready};
use super::{
    Geometry, Memory, Mode, Probe, Sim, ack, design, finish_request, quantum, read, read_hold,
    release_read, release_write, reset, start_request, write, write_hold,
};
use rt_fpga_rtl::host::semantic_registers::*;
use shoop_wasm_test_support::shoop_test;

fn metadata(
    s: &mut Sim<'_>,
    space: u32,
    address: u32,
    value: Option<[u32; 5]>,
    tag: u32,
) -> [u32; 12] {
    let mut payload = [0; 12];
    payload[0] = space | (u32::from(value.is_some()) << 8);
    payload[1] = address;
    if let Some(value) = value {
        payload[2..7].copy_from_slice(&value);
    }
    start_request(s, SEMANTIC_SERVICE_STORE_CONFIG, &payload, tag);
    finish_request(s, tag)
}

#[shoop_test(no_wasm = "Native public clear, actual private loader/media and DDR ownership")]
fn public_clear_view_is_complete_private_full_width_and_never_holds_audio() {
    let root = rt_fpga_testbench::root();
    let mut d = design();
    d.probes
        .extend(["store_cmd_valid", "store_response_ready"].map(Probe::observe));
    let compiled = d
        .build(&root.join("work/marlin-suite/semantic-mmio-audio"))
        .unwrap();
    let mut s = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
    let geometry = Geometry::new([8, 64, 32], 2);
    let memory = Memory::attach(
        &mut s,
        Mode::Fixed,
        geometry.words() + 128,
        0x0100_0000,
        &root.join("work/marlin-suite/semantic-mmio-audio/memory"),
        113,
    )
    .unwrap();
    memory.borrow_mut().control.stalls = true;
    reset(&mut s);
    ack(&mut s, true, 0, 1, 1);
    metadata(&mut s, 1, 0, Some([3, 7, 0, 0, 0]), 40);
    metadata(&mut s, 0, 0, Some([1, 0, 0, 0, 0]), 41);
    assert_eq!(&metadata(&mut s, 1, 0, None, 42)[..5], &[3, 7, 0, 0, 0]);
    assert_eq!(s.model.get("sync_frame"), 0);
    let mut packet = vec![1, SEMANTIC_SERVICE_CLEAR_VIEW];
    packet.extend([0; 12]);
    for (index, word) in packet.into_iter().enumerate() {
        write(&mut s, REG_REQUEST_PUSH, 50);
        assert_eq!(read(&mut s, REG_SUBMISSION_RESULT), RECEIPT_INCOMPLETE);
        assert_eq!(s.model.get("store_cmd_valid"), 0);
        quantum(&mut s, index as u64 * 32, 0, &mut vec![]);
        assert_eq!(s.model.get("store_cmd_valid"), 0);
        write(&mut s, REG_REQUEST_EPOCH + index as u32 * 4, word);
    }
    write_hold(&mut s, REG_REQUEST_PUSH, 50);
    quantum(&mut s, 448, 0, &mut vec![]);
    response_ready(&mut s);
    assert_eq!(read(&mut s, REG_RESPONSE_CODE), 0);
    assert_eq!(
        s.model.get("store_done"),
        0,
        "backend reply already copied and consumed"
    );
    assert_eq!(s.model.get("store_cmd_ready"), 1);
    assert_eq!(s.model.get("s_axi_bvalid"), 1);
    for index in 0..12 {
        assert_eq!(read(&mut s, REG_RESPONSE_DATA + index * 4), 0);
    }
    release_write(&mut s);
    let copied = read_hold(&mut s, REG_RESPONSE_DATA);
    write(&mut s, REG_RESPONSE_POP, 50);
    let mut request = [0; 12];
    request[0] = 1;
    request_writes(&mut s, SEMANTIC_SERVICE_STORE_CONFIG, &request, 51);
    quantum(&mut s, 480, 0, &mut vec![]);
    release_read(&mut s, copied);
    assert_eq!(&finish_request(&mut s, 51)[..5], &[3, 0, 0, 0, 0]);
    let frame = s.model.get("sync_frame");
    let mut bad_view = [0; 12];
    bad_view[1] = 1;
    start_request(&mut s, SEMANTIC_SERVICE_CLEAR_VIEW, &bad_view, 52);
    response_ready(&mut s);
    assert_eq!(
        read(&mut s, REG_RESPONSE_CODE),
        3,
        "full u64 view cannot alias view zero"
    );
    for index in 0..12 {
        assert_eq!(read(&mut s, REG_RESPONSE_DATA + index * 4), 0);
    }
    write(&mut s, REG_RESPONSE_POP, 52);
    assert_eq!(s.model.get("faulted"), 0);
    assert_eq!(s.model.get("sync_frame"), frame);
    assert_eq!(&metadata(&mut s, 1, 0, None, 53)[..5], &[3, 0, 0, 0, 0]);
    let mut bad_reserved = [0; 12];
    bad_reserved[11] = 1;
    start_request(&mut s, SEMANTIC_SERVICE_CLEAR_VIEW, &bad_reserved, 54);
    for _ in 0..32 {
        assert_eq!(s.model.get("store_cmd_valid"), 0);
        s.idle(1);
    }
    response_ready(&mut s);
    assert_eq!(read(&mut s, REG_RESPONSE_CODE), 2);
    write(&mut s, REG_RESPONSE_POP, 54);
    quantum(&mut s, 512, 0, &mut vec![]);
}
