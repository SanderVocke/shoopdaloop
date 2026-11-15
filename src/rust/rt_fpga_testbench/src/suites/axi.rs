//! AXI endpoint, word/chunk adapter and native-memory conformance.
use crate::{
    Result,
    drivers::endpoint::{self as ep, BASE, Endpoint, WORDS},
    memory::{Backend, Memory, Mode},
    sim::Sim,
};

pub fn run(width: u64, mode: Mode) -> Result<()> {
    let compiled = ep::design("axi_master", width, mode)
        .build(&crate::root().join("work/marlin-suite/axi"))?;
    for test in [words_and_bursts, independent_channels_and_reset] {
        let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
        let mut d = Endpoint::new(&mut s, mode, false, false)?;
        test(&mut d, &mut s);
        d.memory.borrow_mut().report(true);
    }
    let compiled =
        ep::design("word_axi", width, mode).build(&crate::root().join("work/marlin-suite/axi"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
    let mut d = Endpoint::new(&mut s, mode, true, false)?;
    word_adapter(&mut d, &mut s);
    d.memory.borrow_mut().report(true);
    drop(s);
    let compiled =
        ep::design("chunk_axi", width, mode).build(&crate::root().join("work/marlin-suite/axi"))?;
    for test in [
        packing_splitting_and_backpressure,
        bounds_fault_drain_and_reset,
    ] {
        let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
        let mut d = Endpoint::new(&mut s, mode, true, true)?;
        test(&mut d, &mut s);
        d.memory.borrow_mut().report(true);
    }
    println!("axi/{width}/{mode:?}: 5 regression groups passed");
    Ok(())
}
fn words_and_bursts(d: &mut Endpoint, s: &mut Sim<'_>) {
    d.reset(s);
    let width = d.memory.borrow().width;
    let mut oracle: Vec<_> = Memory::read(&d.memory, s)
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    for count in [1, 2, 16] {
        for size in 0..=width.ilog2() {
            for shift in [0, 128] {
                let mut address =
                    BASE + if count * (1u64 << size) <= 16 {
                        4080
                    } else {
                        4096
                    } + shift;
                if count == 1 && (1 << size) < width {
                    address += (width - (1 << size)) as u64;
                }
                let mut data = Vec::new();
                let mut strobes = Vec::new();
                for i in 0..count {
                    let lane = (address + (i << size)) % width as u64;
                    let mask = ((1u64 << (1u64 << size)) - 1) << lane;
                    let strobe = mask & d.random.next() & ((1 << width) - 1);
                    let payload = d.random.next()
                        & if width == 8 {
                            u64::MAX
                        } else {
                            u32::MAX as u64
                        };
                    data.push(payload);
                    strobes.push(strobe);
                    let offset = address + (i << size) - BASE - lane;
                    for byte in 0..width {
                        if strobe & (1 << byte) != 0 {
                            oracle[offset as usize + byte] = (payload >> (8 * byte)) as u8;
                        }
                    }
                }
                d.transaction(
                    s,
                    address,
                    u64::from(size),
                    count,
                    Some(&data),
                    &strobes,
                    false,
                );
                let result = d.transaction(s, address, u64::from(size), count, None, &[], false);
                for (i, value) in result.into_iter().enumerate() {
                    let offset = (address + ((i as u64) << size) - BASE) as usize;
                    let aligned = offset - offset % width;
                    let wanted = oracle[aligned..aligned + width]
                        .iter()
                        .enumerate()
                        .fold(0, |v, (i, &b)| v | (u64::from(b) << (i * 8)));
                    assert_eq!(value, wanted);
                }
            }
        }
    }
    assert_eq!(
        Memory::read(&d.memory, s)
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>(),
        oracle
    );
    for write in [false, true] {
        d.memory.borrow_mut().control.fail_next = true;
        d.transaction(s, BASE, 2, 1, write.then_some(&[123][..]), &[15], true);
        assert!(!d.memory.borrow().control.fail_next);
    }
    for (address, size, count) in [
        (BASE + 1, 2, 1),
        (BASE + 4092, 2, 2),
        (BASE, 2, 17),
        (BASE, 4, 1),
    ] {
        for write in [false, true] {
            let requests = d.memory.borrow().requests();
            d.transaction(
                s,
                address,
                size,
                count,
                write.then_some(vec![0; count as usize]).as_deref(),
                &vec![0; count as usize],
                true,
            );
            assert_eq!(d.memory.borrow().requests(), requests);
        }
    }
    if let Backend::Axi(a) = &d.memory.borrow().backend {
        for write in [false, true] {
            let expected: u64 = a
                .records
                .iter()
                .filter(|r| r.write == write)
                .map(|r| {
                    assert_eq!(
                        r.beat_cycles.len() as u64,
                        if write { 1 } else { r.length + 1 }
                    );
                    let native = a.timing.native_bytes();
                    (r.address + ((r.length + 1) << r.size) - 1) / native - r.address / native + 1
                })
                .sum();
            assert_eq!(
                expected,
                if write {
                    a.native_writes
                } else {
                    a.native_reads
                }
            );
        }
    }
}
fn independent_channels_and_reset(d: &mut Endpoint, s: &mut Sim<'_>) {
    d.reset(s);
    d.memory.borrow_mut().control.stalls = false;
    for hold_address in [true, false] {
        {
            let mut m = d.memory.borrow_mut();
            m.control.hold_aw = hold_address;
            m.control.hold_w = !hold_address;
        }
        s.model.set_all(&[
            ("command_valid", 1),
            ("command_write", 1),
            ("command_address", BASE),
            ("command_size", 2),
            ("command_length", 0),
        ]);
        assert_ne!(d.tick(s).get("command_ready"), 0);
        s.model.set_all(&[
            ("command_valid", 0),
            ("write_valid", 1),
            ("write_data", 0xdeadbeef),
            ("write_strobe", 15),
            ("completion_ready", 0),
        ]);
        let mut sent = false;
        for _ in 0..8 {
            if d.tick(s).get("write_ready") != 0 {
                sent = true;
                s.model.set("write_valid", 0);
            }
        }
        assert_eq!(sent, hold_address);
        assert_eq!(d.memory.borrow().active().is_none(), hold_address);
        {
            let mut m = d.memory.borrow_mut();
            m.control.hold_aw = false;
            m.control.hold_w = false;
        }
        s.model.set("write_valid", u64::from(!sent));
        let mut done = false;
        for _ in 0..1000 {
            let b = d.tick(s);
            if b.get("write_ready") != 0 {
                s.model.set("write_valid", 0);
            }
            if b.get("completion_valid") != 0 {
                done = true;
                break;
            }
        }
        assert!(done);
        for _ in 0..5 {
            assert_ne!(d.tick(s).get("completion_valid"), 0);
        }
        s.model.set("completion_ready", 1);
        d.tick(s);
    }
    d.memory.borrow_mut().control.pause_responses = true;
    s.model.set_all(&[
        ("completion_ready", 0),
        ("command_valid", 1),
        ("command_write", 0),
    ]);
    assert_ne!(d.tick(s).get("command_ready"), 0);
    s.model.set("command_valid", 0);
    for _ in 0..8 {
        d.tick(s);
    }
    assert!(d.memory.borrow().active().is_some());
    d.reset(s);
    assert!(d.memory.borrow().active().is_none());
    d.memory.borrow_mut().control.pause_responses = false;
    assert_eq!(
        d.transaction(s, BASE, 2, 1, None, &[], false)[0] as u32,
        0xdeadbeef
    );
}
fn word_adapter(d: &mut Endpoint, s: &mut Sim<'_>) {
    d.memory.borrow_mut().word_only();
    d.reset(s);
    for offset in [0, 4, 12, 28, 4092, 4096, (WORDS * 4 - 4) as i64] {
        let value = d.random.next() as u32;
        d.word_access(s, offset, Some(value), false);
        assert_eq!(d.word_access(s, offset, None, false), value);
    }
    for write in [false, true] {
        d.memory.borrow_mut().control.fail_next = true;
        d.word_access(s, 4, write.then_some(71), true);
    }
    for offset in [-4, 1, (WORDS * 4) as i64] {
        let count = d.memory.borrow().requests();
        d.word_access(s, offset, None, true);
        assert_eq!(d.memory.borrow().requests(), count);
    }
    s.model.set("physical_base", 0xfffffffc);
    d.word_access(s, 4, None, true);
}
fn packing_splitting_and_backpressure(d: &mut Endpoint, s: &mut Sim<'_>) {
    d.reset(s);
    d.logical_base = 0x1234000124;
    s.model.set("arena_base", d.logical_base);
    let mut oracle = Memory::read(&d.memory, s);
    let width = d.memory.borrow().width;
    for (offset, count) in [
        (0, 1),
        (4, 2),
        (0, 3),
        (4, 16),
        (0, 17),
        (4, 31),
        (0, 32),
        (0, 33),
        (4, 64),
        (4092, 64),
        (4096, 1031),
        ((WORDS * 4 - 132) as i64, 33),
    ] {
        let data: Vec<_> = (0..count).map(|_| d.random.next() as u32).collect();
        let start = if let Backend::Axi(a) = &d.memory.borrow().backend {
            a.records.len()
        } else {
            d.memory.borrow().requests() as usize
        };
        d.chunk_access(s, offset, count, Some(&data), false);
        oracle[offset as usize / 4..offset as usize / 4 + count as usize].copy_from_slice(&data);
        assert_eq!(
            Memory::read(&d.memory, s),
            oracle,
            "write changed adjacent lanes"
        );
        assert_eq!(d.chunk_access(s, offset, count, None, false), data);
        if let Backend::Axi(a) = &d.memory.borrow().backend {
            for write in [false, true] {
                let rows: Vec<_> = a.records[start..]
                    .iter()
                    .filter(|r| r.write == write)
                    .collect();
                assert_eq!(
                    rows.iter().map(|r| (r.length + 1) << r.size).sum::<u64>(),
                    count * 4
                );
                let mut address = BASE + offset as u64;
                for r in &rows {
                    assert_eq!(r.address, address);
                    address += (r.length + 1) << r.size;
                    assert!(r.length < 16);
                    assert_eq!(r.address / 4096, (address - 1) / 4096);
                }
                if offset == 0 && count == 32 {
                    assert_eq!(rows.len(), if width == 8 { 1 } else { 2 });
                    assert!(
                        rows.iter()
                            .all(|r| r.length == 15 && r.size == if width == 8 { 3 } else { 2 })
                    );
                }
            }
        } else if offset == 0 && count == 32 {
            assert_eq!(
                d.memory.borrow().requests() - start as u64,
                if width == 8 { 2 } else { 4 }
            );
        }
    }
    for hold in [true, false] {
        let data: Vec<_> = (0..32).map(|_| d.random.next() as u32).collect();
        {
            let mut m = d.memory.borrow_mut();
            m.control.hold_aw = hold;
            m.control.hold_w = !hold;
        }
        d.begin(s, 0, 32, true);
        d.finish(s, 32, Some(&data), false, None, true);
        assert_eq!(d.chunk_access(s, 0, 32, None, false), data);
    }
}
fn bounds_fault_drain_and_reset(d: &mut Endpoint, s: &mut Sim<'_>) {
    d.reset(s);
    for (offset, count) in [
        (0, 0),
        (-4, 1),
        (1, 1),
        ((WORDS * 4 - 4) as i64, 2),
        (0, 0xffffffff),
    ] {
        for write in [false, true] {
            let requests = d.memory.borrow().requests();
            d.chunk_access(s, offset, count, write.then_some(&[][..]), true);
            assert_eq!(d.memory.borrow().requests(), requests);
        }
    }
    s.model.set("physical_base", 0xfffffff8);
    d.chunk_access(s, 0, 3, None, true);
    d.reset(s);
    s.model.set_all(&[
        ("arena_base", u64::MAX - 3),
        ("arena_bytes", 16),
        ("memory_valid", 1),
        ("memory_write", 0),
        ("memory_address", u64::MAX - 3),
        ("memory_count", 2),
    ]);
    assert_ne!(d.tick(s).get("memory_ready"), 0);
    s.model.set("memory_valid", 0);
    d.finish(s, 2, None, true, None, false);
    d.reset(s);
    for write in [false, true] {
        let requests = d.memory.borrow().requests();
        d.memory.borrow_mut().control.fail_next = true;
        d.begin(s, 0, 64, write);
        d.finish(s, 64, write.then_some(&[91; 64][..]), true, Some(0), false);
        assert_eq!(
            d.memory.borrow().requests(),
            requests + 1,
            "issued bursts after failure"
        );
    }
    if !d.memory.borrow().native() {
        d.memory.borrow_mut().control.error_index = Some(3);
        let prefix = 3 * (d.memory.borrow().width / 4);
        d.begin(s, 0, 64, false);
        let got = d.finish(s, 64, None, true, Some(prefix), false);
        assert_eq!(got, Memory::read(&d.memory, s)[..prefix]);
        d.memory.borrow_mut().control.error_index = None;
    }
    assert_eq!(
        d.chunk_access(s, 0, 32, None, false),
        Memory::read(&d.memory, s)[..32]
    );
    for (write, packed) in [(false, false), (true, false), (true, true)] {
        d.memory.borrow_mut().control.pause_responses = true;
        d.begin(s, 0, 32, write);
        let mut sent = 0;
        for _ in 0..200 {
            let offered = write && (packed || sent == 0);
            s.model.set_all(&[
                ("memory_write_valid", u64::from(offered)),
                ("memory_write_data", 0xabcdef01),
            ]);
            if d.tick(s).get("memory_write_ready") != 0 && offered {
                sent += 1;
            }
        }
        assert!(d.memory.borrow().active().is_some());
        d.reset(s);
        assert!(d.memory.borrow().active().is_none());
        d.memory.borrow_mut().control.pause_responses = false;
        let data: Vec<_> = (0..32).map(|_| d.random.next() as u32).collect();
        d.chunk_access(s, 0, 32, Some(&data), false);
        assert_eq!(d.chunk_access(s, 0, 32, None, false), data);
    }
}
