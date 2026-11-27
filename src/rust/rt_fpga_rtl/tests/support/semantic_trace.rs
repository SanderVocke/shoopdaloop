use super::*;
use rt_fpga_rtl::bits::Bits;

fn record(s: &mut Sim<'_>, epoch: u32, seed: u32) -> [u32; 11] {
    let revision = 0xfedc_ba98_7654_0000 | u64::from(seed);
    let raw: [u32; 8] = core::array::from_fn(|i| 0x8123_4000 | (seed << 4) | i as u32);
    s.model.set_all(&[
        ("trace_valid", 1),
        ("trace_epoch", u64::from(epoch)),
        ("trace_revision", revision),
    ]);
    s.model.set_bits("trace_data", &Bits::from(raw));
    s.model.eval();
    let mut words = [0; 11];
    words[..3].copy_from_slice(&[epoch, revision as u32, (revision >> 32) as u32]);
    words[3..].copy_from_slice(&raw);
    words
}
fn hold(s: &mut Sim<'_>, index: u32, consume: bool) -> u32 {
    s.model.set_all(&[
        ("s_axi_araddr", u64::from(REG_TRACE_DATA + 4 * index)),
        ("s_axi_arvalid", 1),
    ]);
    let before = s.tick(&["s_axi_arready", "trace_ready"]).0;
    assert_eq!(before.get("s_axi_arready"), 1);
    assert_eq!(before.get("trace_ready"), u64::from(consume));
    s.model
        .set_all(&[("s_axi_arvalid", 0), ("s_axi_araddr", 0xfff)]);
    assert_eq!(s.model.get("s_axi_rvalid"), 1);
    assert_eq!(s.model.get("s_axi_rresp"), 0);
    s.model.get("s_axi_rdata") as u32
}
fn word(s: &mut Sim<'_>, index: u32, consume: bool) -> u32 {
    let copied = hold(s, index, consume);
    let (response, value) = finish_read(s);
    assert_eq!((response, value), (0, copied));
    copied
}
fn complete(s: &mut Sim<'_>, expected: &[u32; 11]) {
    for (index, expected) in expected.iter().enumerate() {
        assert_eq!(word(s, index as u32, index == 10), *expected);
    }
    s.model.set("trace_valid", 0);
    s.idle(1);
    assert_eq!(read(s, REG_TRACE_STATUS), 0);
    assert_eq!(read(s, REG_TRACE_RECEIPT), RECEIPT_ACCEPTED);
}

#[shoop_test]
fn optional_trace_copies_complete_records_and_never_consumes_a_reset_or_reused_prefix() {
    run(|s| {
        for address in [
            REG_TRACE_STATUS,
            REG_TRACE_LOST_LOW,
            REG_TRACE_LOST_HIGH,
            REG_TRACE_RECEIPT,
            REG_TRACE_DATA,
        ] {
            ar(s, address);
            assert_eq!(finish_read(s), (2, 0), "disabled trace stays unmapped");
            assert_eq!(s.model.get("trace_ready"), 0);
        }
    });
    run_profile(true, |s| {
        assert_eq!(SEMANTIC_TRACE_WORDS, 11);
        assert_eq!(word(s, 0, false), 0);
        assert_eq!(read(s, REG_TRACE_RECEIPT), RECEIPT_UNAVAILABLE);
        let first = record(s, 1, 1);
        s.model.set("trace_lost", 0xfedc_ba98_7654_3210);
        assert_eq!(read(s, REG_TRACE_LOST_LOW), 0x7654_3210);
        assert_eq!(read(s, REG_TRACE_LOST_HIGH), 0xfedc_ba98);
        assert_eq!(read(s, REG_TRACE_STATUS), 1);
        assert_eq!(word(s, 10, false), 0);
        assert_eq!(read(s, REG_TRACE_RECEIPT), RECEIPT_INCOMPLETE);
        ar(s, REG_TRACE_DATA + 1);
        assert_eq!(finish_read(s), (2, 0));
        assert_eq!(word(s, 0, false), first[0]);
        assert_eq!(word(s, 1, false), first[1]);
        assert_eq!(
            word(s, 0, false),
            first[0],
            "header rewinds the same held record"
        );
        for index in 1..10 {
            assert_eq!(word(s, index, false), first[index as usize]);
        }
        let copied = hold(s, 10, true);
        assert_eq!(copied, first[10]);
        let next = record(s, 1, 2);
        s.idle(16);
        assert_eq!(s.model.get("s_axi_rdata"), u64::from(copied));
        assert_eq!(s.model.get("trace_ready"), 0);
        assert_eq!(finish_read(s), (0, copied));
        assert_eq!(
            word(s, 10, false),
            0,
            "repeated final read cannot consume replacement"
        );
        assert_eq!(read(s, REG_TRACE_RECEIPT), RECEIPT_INCOMPLETE);
        complete(s, &next);
        let mut epoch = 1;
        for prefix in 0usize..=11 {
            let old = record(s, epoch, 3 + prefix as u32);
            for (index, expected) in old.iter().take(prefix.saturating_sub(1)).enumerate() {
                assert_eq!(word(s, index as u32, false), *expected);
            }
            let copied = if prefix == 0 {
                None
            } else {
                Some(hold(s, prefix as u32 - 1, prefix == 11))
            };
            if let Some(copied) = copied {
                assert_eq!(copied, old[prefix - 1]);
            }
            s.model.set("runtime_reset", 1);
            s.idle(1);
            epoch += 1;
            s.model.set("epoch", u64::from(epoch));
            let fresh = record(s, epoch, 32 + prefix as u32);
            s.idle(2);
            assert_eq!(s.model.get("trace_ready"), 0);
            s.model.set("runtime_reset", 0);
            s.idle(2);
            if let Some(copied) = copied {
                assert_eq!(
                    finish_read(s),
                    (0, copied),
                    "copied R survives reset at prefix {prefix}"
                );
            }
            assert_eq!(word(s, prefix.clamp(1, 10) as u32, false), 0);
            assert_eq!(read(s, REG_TRACE_RECEIPT), RECEIPT_INCOMPLETE);
            complete(s, &fresh);
        }
        record(s, epoch + 1, 63);
        assert_eq!(read(s, REG_TRACE_STATUS), 0);
        assert_eq!(word(s, 0, false), 0);
        assert_eq!(read(s, REG_TRACE_RECEIPT), RECEIPT_EPOCH);
        let retained = record(s, epoch, 64);
        for (index, expected) in retained.iter().take(10).enumerate() {
            assert_eq!(word(s, index as u32, false), *expected);
        }
        s.model.set("pipeline_fault", 1);
        s.idle(2);
        assert_eq!(s.model.get("faulted"), 1);
        assert_eq!(word(s, 10, false), retained[10]);
        assert_eq!(read(s, REG_TRACE_RECEIPT), RECEIPT_FAULT);
        assert_eq!(
            read(s, REG_TRACE_STATUS),
            1,
            "terminal diagnostics stay retained"
        );
        assert_eq!(
            s.model.get("renders"),
            0,
            "reads do not manufacture processing"
        );
    });
}
