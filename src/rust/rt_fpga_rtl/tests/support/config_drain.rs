use super::{access, data, fill, pop, request, reset, run, wait_reply};
use shoop_wasm_test_support::shoop_test;

fn enable(s: &mut super::Sim<'_>) {
    fill(s, &request(9, 1, 0, [0; 5]));
    assert_eq!(access(s, 1, 0, 91), 0);
    wait_reply(s);
    pop(s, 91);
    assert_eq!(s.model.get("audio_enable"), 1);
}

#[shoop_test]
fn drain_validates_every_word_before_stop_and_copies_a_real_fence_reply_locally() {
    run(|s| {
        enable(s);
        for index in 2..14 {
            for bit in 0..32 {
                let mut words = request(12, 0, 0, [0; 5]);
                words[index] = 1 << bit;
                fill(s, &words);
                assert_eq!(access(s, 1, 0, 92), 0);
                wait_reply(s);
                assert_eq!(s.model.get("response_code"), 2);
                assert_eq!(data(s), [0; 12]);
                assert_eq!(s.model.get("audio_enable"), 1);
                assert_eq!(s.model.get("config_valid"), 0);
                pop(s, 92);
            }
        }
        for (index, word) in request(12, 0, 0, [0; 5]).iter().enumerate() {
            assert_eq!(access(s, 1, 0, 93), 7);
            s.idle(3);
            assert_eq!(s.model.get("audio_enable"), 1);
            assert_eq!(s.model.get("config_valid"), 0);
            assert_eq!(access(s, 0, index as u64, *word), 0);
        }
        assert_eq!(access(s, 1, 0, 93), 0);
        s.idle(13);
        assert_eq!(s.model.get("audio_enable"), 1);
        s.idle(1);
        assert_eq!(s.model.get("audio_enable"), 0);
        for _ in 0..40 {
            assert_eq!(s.model.get("config_valid"), 1);
            assert_eq!(s.model.get("config_operation"), 7);
            assert_eq!(
                s.model.bits("config_media_data"),
                rt_fpga_rtl::bits::Bits::zero(224)
            );
            assert_eq!(s.model.get("response_pending"), 0);
            s.idle(1);
        }
        s.model.set("config_ready", 1);
        s.idle(1);
        s.model.set("config_ready", 0);
        assert_eq!(s.model.get("config_valid"), 0);
        s.idle(40);
        assert_eq!(s.model.get("response_pending"), 0, "not idle-as-fence");
        s.model.set_bits(
            "config_store_data",
            &rt_fpga_rtl::bits::Bits::from([u32::MAX; 5]),
        );
        s.model.set("config_response_valid", 1);
        assert_eq!(s.model.get("config_response_ready"), 1);
        s.idle(1);
        s.model.set("config_response_valid", 0);
        wait_reply(s);
        assert_eq!(s.model.get("response_code"), 0);
        assert_eq!(data(s), [0; 12]);
        s.idle(40);
        assert_eq!(s.model.get("audio_enable"), 0);
        assert_eq!(s.model.get("config_valid"), 0);
        pop(s, 93);
        fill(s, &request(9, 1, 0, [0; 5]));
        assert_eq!(access(s, 1, 0, 94), 0);
        wait_reply(s);
        assert_eq!(s.model.get("response_code"), 2);
        assert_eq!(s.model.get("audio_enable"), 0);
    });
}

#[shoop_test]
fn drain_halt_and_backend_error_preserve_stop_without_success_replay_or_cancellation() {
    run(|s| {
        for position in 0..19 {
            reset(s);
            enable(s);
            fill(s, &request(12, 0, 0, [0; 5]));
            assert_eq!(access(s, 1, 0, 95), 0);
            s.idle(position);
            s.model.set("halt", 1);
            s.model.eval();
            assert_eq!(s.model.get("audio_enable"), 0);
            wait_reply(s);
            assert_eq!(s.model.get("response_code"), 3);
            assert_eq!(data(s), [0; 12]);
            s.model
                .set_all(&[("config_ready", 1), ("config_response_valid", 1)]);
            s.idle(40);
            assert_eq!(s.model.get("config_valid"), 0);
            assert_eq!(s.model.get("config_response_ready"), 0);
            assert_eq!(s.model.get("response_code"), 3);
        }
        reset(s);
        enable(s);
        fill(s, &request(12, 0, 0, [0; 5]));
        assert_eq!(access(s, 1, 0, 96), 0);
        s.until(32, "validated stop before endpoint grant", |m| {
            m.get("audio_enable") == 0
        });
        s.model.set("config_ready", 1);
        s.idle(1);
        s.model.set_all(&[
            ("config_ready", 0),
            ("config_response_valid", 1),
            ("config_response_error", 1),
        ]);
        s.idle(1);
        s.model.set("config_response_valid", 0);
        wait_reply(s);
        assert_eq!(s.model.get("response_code"), 3);
        assert_eq!(data(s), [0; 12]);
        assert_eq!(s.model.get("audio_enable"), 0);
        s.idle(50);
        assert_eq!(s.model.get("config_valid"), 0);
        reset(s);
        s.idle(40);
        assert_eq!(s.model.get("audio_enable"), 0);
        assert_eq!(s.model.get("response_pending"), 0);
    });
}
