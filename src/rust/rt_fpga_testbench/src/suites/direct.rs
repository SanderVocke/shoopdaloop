//! Direct AXI slave checks without an endpoint translating or validating requests.
use crate::{
    Result,
    memory::{Memory, Mode, Shared, Stable},
    model::Design,
    random::Random,
    sim::Sim,
};
const BASE: u64 = 0x01000000;
const WORDS: usize = 2048;
const SIGNALS: &[&str] = &[
    "m_axi_awready",
    "m_axi_wready",
    "m_axi_bvalid",
    "m_axi_bresp",
    "m_axi_arready",
    "m_axi_rvalid",
    "m_axi_rdata",
    "m_axi_rresp",
    "m_axi_rlast",
];
pub fn run(width: u64) -> Result<()> {
    let compiled = Design::new(
        "axi_direct_memory",
        vec![crate::root().join("fpga/axi_memory/tests/axi_direct_memory.sv")],
    )
    .parameters(&[("DATA_WIDTH", width), ("WORDS", WORDS as u64)])
    .administration(WORDS)
    .build(&crate::root().join("work/marlin-suite/direct"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
    let memory = Memory::attach(
        &mut s,
        Mode::Instant,
        WORDS,
        BASE,
        &crate::root().join("work/marlin-suite/direct"),
        913,
    )?;
    s.model
        .set_all(&[("reset", 1), ("m_axi_awburst", 1), ("m_axi_arburst", 1)]);
    s.idle(1);
    s.model.set("reset", 0);
    s.idle(1);
    let mut random = Random::new(913);
    let words: Vec<_> = (0..WORDS).map(|_| random.next() as u32).collect();
    Memory::load(&memory, &mut s, &words);
    assert_eq!(Memory::read(&memory, &mut s), words);
    let mut oracle: Vec<_> = words.into_iter().flat_map(u32::to_le_bytes).collect();
    let bytes = width / 8;
    for size in 0..=bytes.ilog2() {
        for count in [1, 2, 16] {
            let start = BASE + 4096 + if count == 1 { bytes - (1 << size) } else { 0 };
            let data: Vec<_> = (0..count)
                .map(|_| {
                    random.next()
                        & if width == 64 {
                            u64::MAX
                        } else {
                            u32::MAX as u64
                        }
                })
                .collect();
            let mut strobes = Vec::new();
            for (i, &payload) in data.iter().enumerate() {
                let address = start + ((i as u64) << size);
                let lane = address % bytes;
                let strobe =
                    (((1u64 << (1u64 << size)) - 1) << lane) & random.next() & ((1 << bytes) - 1);
                strobes.push(strobe);
                let offset = (address - BASE - lane) as usize;
                for byte in 0..bytes as usize {
                    if strobe & (1 << byte) != 0 {
                        oracle[offset + byte] = (payload >> (byte * 8)) as u8;
                    }
                }
            }
            write(
                &memory,
                &mut s,
                start,
                u64::from(size),
                &data,
                &strobes,
                false,
                count == 2,
                count == 16,
            );
            let actual = read(&mut s, start, u64::from(size), count, false);
            for (i, payload) in actual.into_iter().enumerate() {
                let offset = start + ((i as u64) << size) - BASE;
                let first = (offset - offset % bytes) as usize;
                let wanted = oracle[first..first + bytes as usize]
                    .iter()
                    .enumerate()
                    .fold(0, |v, (i, &b)| v | (u64::from(b) << (i * 8)));
                assert_eq!(payload, wanted);
            }
        }
    }
    let image = |s: &mut Sim<'_>| {
        Memory::read(&memory, s)
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>()
    };
    assert_eq!(image(&mut s), oracle);
    for writing in [false, true] {
        memory.borrow_mut().control.fail_next = true;
        if writing {
            write(&memory, &mut s, BASE, 2, &[123], &[15], true, false, false);
        } else {
            read(&mut s, BASE, 2, 16, true);
        }
        assert!(!memory.borrow().control.fail_next);
    }
    read(&mut s, BASE + (WORDS * 4) as u64, 2, 1, true);
    assert_eq!(image(&mut s), oracle);
    s.model.set_all(&[
        ("m_axi_araddr", BASE),
        ("m_axi_arlen", 15),
        ("m_axi_arsize", 2),
        ("m_axi_arvalid", 1),
        ("m_axi_rready", 0),
    ]);
    assert_ne!(s.tick(SIGNALS).0.get("m_axi_arready"), 0);
    s.model.set("m_axi_arvalid", 0);
    s.idle(5);
    assert_ne!(s.model.get("m_axi_rvalid"), 0);
    s.model.set("reset", 1);
    s.idle(1);
    s.model.set("reset", 0);
    for _ in 0..5 {
        s.idle(1);
        assert_eq!(s.model.get("m_axi_rvalid") | s.model.get("sim_busy"), 0);
    }
    assert_eq!(image(&mut s), oracle);
    println!("direct/{width}: functional memory, strobes, independent channels and reset passed");
    Ok(())
}
fn write(
    memory: &Shared,
    s: &mut Sim<'_>,
    address: u64,
    size: u64,
    data: &[u64],
    strobes: &[u64],
    error: bool,
    hold_address: bool,
    hold_data: bool,
) {
    {
        let mut m = memory.borrow_mut();
        m.control.hold_aw = hold_address;
        m.control.hold_w = hold_data;
    }
    s.model.set_all(&[
        ("m_axi_awaddr", address),
        ("m_axi_awlen", data.len() as u64 - 1),
        ("m_axi_awsize", size),
        ("m_axi_awvalid", 1),
        ("m_axi_bready", 0),
    ]);
    let mut sent = 0;
    let mut admitted = false;
    let mut done = false;
    for cycle in 0..1000 {
        if cycle == 12 {
            let mut m = memory.borrow_mut();
            m.control.hold_aw = false;
            m.control.hold_w = false;
        }
        let offered = sent < data.len();
        s.model.set_all(&[
            ("m_axi_wvalid", u64::from(offered)),
            ("m_axi_wdata", if offered { data[sent] } else { 0 }),
            ("m_axi_wstrb", if offered { strobes[sent] } else { 0 }),
            ("m_axi_wlast", u64::from(sent + 1 == data.len())),
        ]);
        let before = s.tick(SIGNALS).0;
        if !admitted && before.get("m_axi_awready") != 0 {
            admitted = true;
            s.model.set("m_axi_awvalid", 0);
        }
        if offered && before.get("m_axi_wready") != 0 {
            sent += 1;
        }
        if before.get("m_axi_bvalid") != 0 {
            assert!(admitted);
            assert_eq!(sent, data.len());
            assert_eq!(before.get("m_axi_bresp") != 0, error);
            done = true;
            break;
        }
    }
    assert!(done);
    s.model.set("m_axi_wvalid", 0);
    for _ in 0..5 {
        let b = s.tick(SIGNALS).0;
        assert_ne!(b.get("m_axi_bvalid"), 0);
        assert_eq!(b.get("m_axi_bresp") != 0, error);
    }
    s.model.set("m_axi_bready", 1);
    s.idle(1);
    s.model.set("m_axi_bready", 0);
}
fn read(s: &mut Sim<'_>, address: u64, size: u64, count: u64, error: bool) -> Vec<u64> {
    s.model.set_all(&[
        ("m_axi_araddr", address),
        ("m_axi_arlen", count - 1),
        ("m_axi_arsize", size),
        ("m_axi_arvalid", 1),
    ]);
    let mut accepted = false;
    for _ in 0..1000 {
        if s.tick(SIGNALS).0.get("m_axi_arready") != 0 {
            accepted = true;
            break;
        }
    }
    assert!(accepted);
    s.model.set("m_axi_arvalid", 0);
    let mut received = Vec::new();
    let mut held = Stable::default();
    for cycle in 0..1000 {
        let ready = cycle % 5 == 0;
        s.model.set("m_axi_rready", u64::from(ready));
        let before = s.tick(SIGNALS).0;
        let payload = (
            before.get("m_axi_rdata"),
            before.get("m_axi_rresp"),
            before.get("m_axi_rlast"),
        );
        held.check("direct R", before.get("m_axi_rvalid") != 0, ready, payload);
        if before.get("m_axi_rvalid") != 0 && ready {
            assert_eq!(payload.1 != 0, error);
            received.push(payload.0);
            assert_eq!(payload.2 != 0, received.len() as u64 == count);
            if payload.2 != 0 {
                s.model.set("m_axi_rready", 0);
                return received;
            }
        }
    }
    panic!("direct read timeout");
}
