#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_rtl::{bits::Bits, model::Design, sim::Sim};
use shoop_wasm_test_support::shoop_test;

#[path = "support/config_lifecycle.rs"]
mod lifecycle;
#[path = "support/config_media.rs"]
mod media;

fn run(test: impl FnOnce(&mut Sim<'_>)) {
    let root = rt_fpga_rtl::root();
    let model = Design::new(
        "fifo_config_service",
        vec![root.join("fpga/host_bridge/rtl/fifo_config_service.sv")],
    )
    .build(&root.join("work/config-service"))
    .unwrap();
    let mut s = Sim::new(model.session(None).unwrap(), &[("clk", 10_000, 0)]);
    reset(&mut s);
    test(&mut s);
}
fn reset(s: &mut Sim<'_>) {
    s.model.set_all(&[
        ("live", 0),
        ("halt", 0),
        ("epoch", 1),
        ("valid", 0),
        ("config_ready", 0),
        ("config_response_valid", 0),
        ("config_response_error", 0),
        ("media_valid", 0),
        ("media_operation", 0),
        ("media_word", 0),
        ("config_media_write_ready", 0),
        ("config_media_read_valid", 0),
        ("config_media_result_empty", 0),
        ("config_media_result_view", 0),
    ]);
    s.idle(2);
    s.model.set("live", 1);
    s.idle(1);
}
fn access(s: &mut Sim<'_>, operation: u64, index: u64, word: u32) -> u64 {
    s.model.set_all(&[
        ("valid", 1),
        ("operation", operation),
        ("index", index),
        ("word", u64::from(word)),
    ]);
    s.model.eval();
    let receipt = s.model.get("receipt");
    s.idle(1);
    s.model.set("valid", 0);
    s.model.eval();
    receipt
}
fn request(op: u32, control: u32, address: u32, data: [u32; 5]) -> [u32; 14] {
    let mut words = [0; 14];
    words[..4].copy_from_slice(&[1, op, control, address]);
    words[4..9].copy_from_slice(&data);
    words
}
fn fill(s: &mut Sim<'_>, words: &[u32; 14]) {
    for (i, word) in words.iter().enumerate() {
        assert_eq!(access(s, 0, i as u64, *word), 0);
    }
}
fn data(s: &mut Sim<'_>) -> [u32; 12] {
    core::array::from_fn(|index| {
        s.model.set("read_index", index as u64);
        s.model.eval();
        s.model.get("read_data") as u32
    })
}
fn response_data(s: &mut Sim<'_>, core: [u32; 5], audio: u32, store: [u32; 5]) {
    let mut bits = Bits::zero(129);
    for (index, word) in core[..4].iter().enumerate() {
        bits.set(index * 32, 32, u64::from(*word));
    }
    bits.set(128, 1, u64::from(core[4] & 1));
    s.model.set_bits("config_core_data", &bits);
    s.model.set("config_audio_data", u64::from(audio));
    s.model.set_bits("config_store_data", &Bits::from(store));
}
fn wait_reply(s: &mut Sim<'_>) {
    s.until(32, "complete private reply copy", |m| {
        m.get("response_pending") != 0
    });
}
fn pop(s: &mut Sim<'_>, tag: u32) {
    assert_eq!(s.model.get("response_pending"), 1);
    s.model.set("read_index", 14);
    s.model.eval();
    assert_eq!(s.model.get("read_data"), u64::from(tag));
    assert_eq!(access(s, 2, 0, tag), 0);
    assert_eq!(s.model.get("request_space"), 1);
    assert_eq!(s.model.get("response_pending"), 0);
}

#[shoop_test]
fn complete_private_requests_and_copied_results_release_backend_without_arm_service() {
    run(|s| {
        for (op, control, address, payload) in [
            (1, 0x103, 0xffff, [11, 22, 33, 44, 1]),
            (2, 0x102, u32::MAX, [0xdead_beef, 0, 0, 0, 0]),
            (3, 0x107, u32::MAX, [55, 66, 77, 88, u32::MAX]),
            (4, u32::MAX, 0x8000_0001, [0; 5]),
        ] {
            let words = request(op, control, address, payload);
            for (index, word) in words.iter().enumerate() {
                assert_eq!(access(s, 1, 0, 37), 7, "prefix {index}");
                s.idle(3);
                assert_eq!(s.model.get("config_valid"), 0);
                assert_eq!(s.model.get("response_pending"), 0);
                assert_eq!(access(s, 0, index as u64, *word), 0);
            }
            assert_eq!(access(s, 1, 0, 0), 4);
            assert_eq!(access(s, 1, 0, 37), 0);
            assert_eq!(s.model.get("request_space"), 0);
            for _ in 0..13 {
                assert_eq!(s.model.get("config_valid"), 0);
                s.idle(1);
            }
            assert_eq!(s.model.get("config_valid"), 1);
            for _ in 0..20 {
                assert_eq!(s.model.get("config_operation"), u64::from(op));
                assert_eq!(s.model.get("config_write"), 1);
                assert_eq!(s.model.get("config_space"), u64::from(control & 7));
                assert_eq!(s.model.get("config_address"), u64::from(address));
                let expected = if op == 4 {
                    [control, address, 0, 0, 0]
                } else {
                    payload
                };
                assert_eq!(s.model.bits("config_data"), Bits::from(expected));
                assert_eq!(access(s, 0, 0, 2), 1);
                assert_eq!(access(s, 1, 0, 38), 1);
            }
            s.model.set("config_ready", 1);
            s.idle(1);
            s.model.set("config_ready", 0);
            assert_eq!(s.model.get("config_valid"), 0);
            s.idle(50);
            assert_eq!(s.model.get("response_pending"), 0);
            let core = [0x1234, 0x5678, 0x9abc, 0xdef0, 1];
            let audio = 0xfeed_beef;
            let store = [0x4edc, 0xba98, 0x7654, 0x3210, u32::MAX];
            response_data(s, core, audio, store);
            s.model.set("config_response_valid", 1);
            s.idle(1);
            s.model.set("config_response_valid", 0);
            assert_eq!(s.model.get("response_pending"), 0);
            wait_reply(s);
            assert_eq!(s.model.get("response_code"), 0);
            let mut expected = [0; 12];
            match op {
                1 => expected[..5].copy_from_slice(&core),
                2 => expected[0] = audio,
                3 => expected[..5].copy_from_slice(&store),
                4 => {}
                _ => unreachable!(),
            }
            assert_eq!(data(s), expected);
            for _ in 0..20 {
                assert_eq!(s.model.get("config_valid"), 0);
                assert_eq!(access(s, 0, 0, 1), 1);
                assert_eq!(access(s, 1, 0, 37), 1);
                assert_eq!(access(s, 2, 0, 38), 6);
                assert_eq!(data(s), expected);
            }
            pop(s, 37);
        }
    });
}

#[shoop_test]
fn malformed_values_and_retired_operations_refuse_before_any_backend_effect() {
    run(|s| {
        assert_eq!(access(s, 0, 1, 1), 7);
        assert_eq!(access(s, 0, 14, 1), 4);
        assert_eq!(access(s, 0, 0, 1), 0);
        assert_eq!(
            access(s, 0, 2, 1),
            7,
            "out-of-order field cannot advance fill"
        );
        assert_eq!(access(s, 0, 1, 1), 0);
        assert_eq!(
            access(s, 0, 1, 3),
            7,
            "old field cannot change private payload"
        );
        assert_eq!(access(s, 1, 0, 1), 7);
        assert_eq!(access(s, 3, 0, 1), 4);
        assert_eq!(access(s, 2, 0, 1), 5);
        let mut cases = vec![];
        for op in [
            0, 5, 6, 7, 8, 10, 11, 12, 13, 14, 15, 16, 0x1_0001, 0x1_0004, 0x1_0009,
        ] {
            cases.push((request(op, 0, 0, [0; 5]), 1));
        }
        for op in 1..=3 {
            for bit in 0..32 {
                if [0, 1, 8].contains(&bit) || (op == 3 && bit == 2) {
                    continue;
                }
                cases.push((request(op, 1 << bit, 0, [0; 5]), 2));
            }
            for reserved in 9..14 {
                let mut bad = request(op, 0, 0, [0; 5]);
                bad[reserved] = 1;
                cases.push((bad, 2));
            }
        }
        for reserved in 4..14 {
            let mut bad = request(4, u32::MAX, u32::MAX, [0; 5]);
            bad[reserved] = 1;
            cases.push((bad, 2));
        }
        cases.push((request(1, 0, 0x1_0000, [0; 5]), 2));
        cases.push((request(1, 0, 0, [0, 0, 0, 0, 2]), 2));
        for extra in 1..5 {
            let mut payload = [0; 5];
            payload[extra] = 1;
            cases.push((request(2, 0, 0, payload), 2));
        }
        for (words, code) in cases {
            fill(s, &words);
            assert_eq!(access(s, 1, 0, 1), 0);
            for _ in 0..26 {
                assert_eq!(s.model.get("config_valid"), 0, "{words:?}");
                s.idle(1);
            }
            assert_eq!(s.model.get("response_pending"), 1);
            assert_eq!(s.model.get("response_code"), code, "{words:?}");
            assert_eq!(data(s), [0; 12]);
            pop(s, 1);
        }
        let mut stale = request(1, 0, 0, [0; 5]);
        stale[0] = 2;
        fill(s, &stale);
        assert_eq!(access(s, 1, 0, 1), 2);
        assert_eq!(s.model.get("config_valid"), 0);
        assert_eq!(s.model.get("response_pending"), 0);
        assert_eq!(access(s, 0, 0, 1), 0);
        assert_eq!(
            access(s, 1, 0, 1),
            7,
            "rebegin discards all prior completeness"
        );
    });
}

#[shoop_test]
fn clear_view_reply_readiness_is_local_and_halt_preserves_uncertainty() {
    run(|s| {
        for clocks in 0..18 {
            reset(s);
            fill(s, &request(4, u32::MAX, 0x8000_0001, [0; 5]));
            assert_eq!(access(s, 1, 0, 19), 0);
            s.idle(clocks);
            if s.model.get("config_valid") != 0 {
                assert_eq!(s.model.get("config_operation"), 4);
                assert_eq!(
                    s.model.bits("config_data").get(0, 64),
                    0x8000_0001_ffff_ffff
                );
                assert_eq!(s.model.get("config_response_ready"), 1);
            }
            if clocks == 17 {
                s.model.set("config_ready", 1);
                s.idle(1);
                s.model.set("config_ready", 0);
            }
            s.model
                .set_all(&[("halt", 1), ("config_response_valid", 1)]);
            s.model.eval();
            assert_eq!(s.model.get("config_valid"), 0);
            assert_eq!(s.model.get("config_response_ready"), 0);
            s.idle(1);
            wait_reply(s);
            assert_eq!(s.model.get("response_code"), 3);
            assert_eq!(data(s), [0; 12]);
            s.model.set("halt", 0);
            s.idle(16);
            assert_eq!(s.model.get("config_response_ready"), 0);
            assert_eq!(s.model.get("config_valid"), 0);
            assert_eq!(s.model.get("response_code"), 3);
            assert_eq!(data(s), [0; 12]);
            s.model.set("live", 0);
            s.idle(2);
            s.model.set_all(&[("live", 1), ("epoch", 2)]);
            s.idle(2);
            assert_eq!(s.model.get("response_pending"), 0);
            assert_eq!(access(s, 1, 0, 19), 7);
        }
    });
}

#[shoop_test]
fn same_edge_halt_and_runtime_reset_preserve_uncertainty_without_replay() {
    run(|s| {
        let words = request(3, 0x100, 42, [1, 2, 3, 4, 5]);
        for clocks in 0..17 {
            reset(s);
            fill(s, &words);
            assert_eq!(access(s, 1, 0, 17), 0);
            s.idle(clocks);
            if clocks == 16 {
                s.model.set("config_ready", 1);
                s.idle(1);
                s.model.set("config_ready", 0);
            }
            s.model.set_all(&[("halt", 1), ("config_ready", 1)]);
            s.model.eval();
            assert_eq!(s.model.get("config_valid"), 0);
            s.idle(1);
            wait_reply(s);
            assert_eq!(s.model.get("response_pending"), 1);
            assert_eq!(s.model.get("response_code"), 3);
            assert_eq!(data(s), [0; 12]);
            assert_eq!(access(s, 2, 0, 17), 3);
            s.model
                .set_all(&[("halt", 0), ("config_response_valid", 1)]);
            response_data(s, [9; 5], 9, [9; 5]);
            s.idle(10);
            assert_eq!(s.model.get("response_code"), 3);
            assert_eq!(data(s), [0; 12]);
            assert_eq!(s.model.get("config_valid"), 0);
            s.model.set("live", 0);
            s.idle(2);
            assert_eq!(s.model.get("response_pending"), 0);
            assert_eq!(access(s, 1, 0, 18), 2);
            s.model.set_all(&[("live", 1), ("epoch", 2)]);
            s.idle(2);
            assert_eq!(s.model.get("response_pending"), 0);
            assert_eq!(access(s, 1, 0, 18), 7);
        }
        reset(s);
        fill(s, &words);
        assert_eq!(access(s, 1, 0, 9), 0);
        s.until(24, "complete validated request", |m| {
            m.get("config_valid") != 0
        });
        s.model.set_all(&[
            ("config_ready", 1),
            ("config_response_valid", 1),
            ("config_response_error", 1),
        ]);
        s.idle(1);
        wait_reply(s);
        assert_eq!(s.model.get("response_pending"), 1);
        assert_eq!(s.model.get("response_code"), 3);
        assert_eq!(data(s), [0; 12]);
        s.model.set("config_response_error", 0);
        s.idle(10);
        assert_eq!(s.model.get("response_code"), 3);
        assert_eq!(data(s), [0; 12]);
    });
}
