use super::{Bits, Sim, access, data, fill, pop, request, reset, run, wait_reply};
use shoop_wasm_test_support::shoop_test;

fn packet(opcode: u32, count: u32) -> [u32; 14] {
    [
        1,
        17,
        opcode,
        0x89ab_cdef,
        0x0123_4567,
        0x7654_3210,
        0xfedc_ba98,
        0xfeed_5678,
        0x8000_1234,
        count,
        0,
        0,
        0,
        0,
    ]
}
fn media(s: &mut Sim<'_>, operation: u64, word: u32) -> (u64, u32) {
    s.model.set_all(&[
        ("media_valid", 1),
        ("media_operation", operation),
        ("media_word", u64::from(word)),
    ]);
    s.model.eval();
    let result = (
        s.model.get("media_receipt"),
        s.model.get("media_read_data") as u32,
    );
    assert_eq!(
        s.model.get("config_media_write_valid"),
        u64::from(operation == 1 && result.0 == 0)
    );
    if operation == 2 && result.0 == 0 {
        assert_eq!(s.model.get("config_media_read_ready"), 1);
    }
    s.idle(1);
    s.model.set("media_valid", 0);
    result
}
fn publish(s: &mut Sim<'_>, words: &[u32; 14]) {
    fill(s, words);
    assert_eq!(access(s, 1, 0, 37), 0);
    s.until(24, "complete media descriptor", |m| {
        m.get("config_valid") != 0
    });
}
fn accept(s: &mut Sim<'_>) {
    s.model.set("config_ready", 1);
    s.idle(1);
    s.model.set("config_ready", 0);
}
fn reply(s: &mut Sim<'_>, error: bool) {
    s.model.set_all(&[
        ("config_response_valid", 1),
        ("config_response_error", u64::from(error)),
    ]);
    s.idle(1);
    s.model.set("config_response_valid", 0);
    wait_reply(s);
}

#[shoop_test]
fn full_width_media_descriptors_and_tagged_private_transfers_share_the_reserved_slot() {
    run(|s| {
        for opcode in 0..8 {
            reset(s);
            let words = packet(
                opcode,
                if opcode == 5 || opcode == 6 {
                    32
                } else {
                    u32::MAX
                },
            );
            for (index, word) in words.iter().enumerate() {
                assert_eq!(access(s, 1, 0, 37), 7);
                assert_eq!(s.model.get("config_valid"), 0);
                assert_eq!(media(s, 0, 37).0, 5);
                assert_eq!(access(s, 0, index as u64, *word), 0);
            }
            assert_eq!(access(s, 1, 0, 37), 0);
            s.until(24, "private descriptor ready", |m| {
                m.get("config_valid") != 0
            });
            assert_eq!(s.model.get("config_operation"), 5);
            assert_eq!(s.model.get("config_space"), u64::from(opcode));
            let descriptor: [u32; 7] = words[3..10].try_into().unwrap();
            assert_eq!(s.model.bits("config_media_data"), Bits::from(descriptor));
            assert_eq!(media(s, 0, 38).0, 6);
            assert_eq!(media(s, 1, 0xdead_beef).0, 6);
            assert_eq!(media(s, 0, 37).0, 0);
            accept(s);
            s.model.set("config_media_write_ready", 1);
            if opcode == 5 {
                for word in 0..32 {
                    assert_eq!(media(s, 1, 0x8000_0000 + word).0, 0);
                    assert_eq!(
                        s.model.get("config_media_write_data"),
                        0x8000_0000 + u64::from(word)
                    );
                }
                s.model.set("config_media_write_ready", 0);
            }
            assert_eq!(media(s, 1, 123).0, 5);
            s.model.set_all(&[
                ("config_media_read_valid", 1),
                ("config_media_read_data", 0x1234_5678),
            ]);
            assert_eq!(
                media(s, 2, 0),
                (5, 0),
                "private data cannot precede successful completion"
            );
            assert_eq!(s.model.get("config_media_read_ready"), 0);
            s.model.set_all(&[
                ("config_media_result_view", 0x8012_3456),
                ("config_media_result_empty", u64::from(opcode == 4)),
            ]);
            reply(s, false);
            let mut expected = [0; 12];
            expected[..3].copy_from_slice(&[
                0x8012_3456,
                u32::from(opcode == 4),
                if opcode == 6 { 32 } else { 0 },
            ]);
            assert_eq!(data(s), expected);
            if opcode == 6 {
                assert_eq!(
                    access(s, 2, 0, 37),
                    7,
                    "unread body retains only this service lane"
                );
                for index in 0..32 {
                    s.model.set("config_media_read_data", 0x1234_0000 + index);
                    assert_eq!(media(s, 2, 0), (0, 0x1234_0000 + index as u32));
                }
            } else {
                assert_eq!(media(s, 2, 0), (5, 0));
            }
            s.model.set("config_media_read_valid", 0);
            pop(s, 37);
            assert_eq!(s.model.get("media_status"), 0);
        }
    });
}

#[shoop_test]
fn refused_media_tags_cannot_redirect_a_live_configuration_ram_address() {
    run(|s| {
        for opcode in 1..=3 {
            reset(s);
            let words = request(opcode, 0x100, 0x2222, [1, 0, 0, 0, 0]);
            publish(s, &words);
            for tag in [0, 37, u32::MAX] {
                assert_eq!(media(s, 0, tag).0, 5);
                assert_eq!(s.model.get("config_valid"), 1);
                assert_eq!(s.model.get("config_address"), 0x2222);
                assert_eq!(s.model.bits("config_data"), Bits::from([1, 0, 0, 0, 0]));
                assert_eq!(s.model.get("config_media_write_valid"), 0);
                assert_eq!(s.model.get("config_media_read_ready"), 0);
            }
        }
    });
}

#[shoop_test]
fn malformed_media_packets_have_no_command_or_data_effect_and_error_prefixes_are_not_published() {
    run(|s| {
        let mut cases = vec![];
        let mut alias = packet(0, 0);
        alias[1] = 0x1_0011;
        cases.push((alias, 1));
        for opcode in [8, 0x1_0000, u32::MAX] {
            cases.push((packet(opcode, 0), 2));
        }
        for index in 10..14 {
            let mut words = packet(6, 32);
            words[index] = 1;
            cases.push((words, 2));
        }
        for opcode in [5, 6] {
            for count in [33, u32::MAX] {
                cases.push((packet(opcode, count), 2));
            }
        }
        for (words, code) in cases {
            reset(s);
            s.model.set_all(&[
                ("config_ready", 1),
                ("config_media_write_ready", 1),
                ("config_media_read_valid", 1),
            ]);
            fill(s, &words);
            assert_eq!(access(s, 1, 0, 37), 0);
            for _ in 0..32 {
                assert_eq!(s.model.get("config_valid"), 0, "{words:?}");
                assert_eq!(s.model.get("config_media_write_valid"), 0);
                assert_eq!(s.model.get("config_media_read_ready"), 0);
                s.idle(1);
            }
            assert_eq!(s.model.get("response_code"), code);
            assert_eq!(data(s), [0; 12]);
        }
        reset(s);
        publish(s, &packet(6, 32));
        accept(s);
        s.model.set_all(&[
            ("config_media_read_valid", 1),
            ("config_media_read_data", 0xdead_beef),
        ]);
        reply(s, true);
        assert_eq!(s.model.get("response_code"), 3);
        assert_eq!(data(s), [0; 12]);
        assert_eq!(media(s, 2, 0), (5, 0));
        assert_eq!(
            s.model.get("config_media_read_ready"),
            1,
            "discard only a retired error prefix locally"
        );
        assert_eq!(access(s, 2, 0, 37), 7);
        s.model.set("config_media_read_valid", 0);
        pop(s, 37);
    });
}

#[shoop_test]
fn media_halt_and_runtime_reset_drop_authority_without_replay_or_late_data_acceptance() {
    run(|s| {
        for clocks in 0..19 {
            reset(s);
            fill(s, &packet(5, 32));
            assert_eq!(access(s, 1, 0, 37), 0);
            s.idle(clocks);
            if s.model.get("config_valid") != 0 {
                assert_eq!(media(s, 0, 37).0, 0);
                accept(s);
            }
            s.model
                .set_all(&[("halt", 1), ("config_media_write_ready", 1)]);
            assert_eq!(media(s, 1, 123).0, 3);
            wait_reply(s);
            assert_eq!(s.model.get("response_code"), 3);
            assert_eq!(data(s), [0; 12]);
            s.model
                .set_all(&[("halt", 0), ("config_response_valid", 1)]);
            s.idle(8);
            assert_eq!(s.model.get("config_valid"), 0);
            assert_eq!(media(s, 1, 123).0, 5);
            s.model.set("live", 0);
            assert_eq!(media(s, 1, 123).0, 2);
            s.model
                .set_all(&[("live", 1), ("epoch", 2), ("config_response_valid", 0)]);
            s.idle(2);
            assert_eq!(s.model.get("media_status"), 0);
            assert_eq!(s.model.get("request_space"), 1);
        }
    });
}
