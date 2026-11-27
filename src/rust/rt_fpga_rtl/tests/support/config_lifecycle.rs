use super::{access, data, fill, pop, request, reset, run, wait_reply};
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn audio_enable_is_complete_local_one_way_and_never_waits_for_a_backend_or_reader() {
    run(|s| {
        s.model.set_all(&[
            ("config_ready", 1),
            ("config_response_valid", 1),
            ("config_response_error", 1),
        ]);
        let words = request(9, 1, 0, [0; 5]);
        for (index, word) in words.iter().enumerate() {
            assert_eq!(access(s, 1, 0, 19), 7, "prefix {index}");
            s.idle(5);
            assert_eq!(s.model.get("audio_enable"), 0);
            assert_eq!(s.model.get("config_valid"), 0);
            assert_eq!(access(s, 0, index as u64, *word), 0);
        }
        assert_eq!(access(s, 1, 0, 19), 0);
        for _ in 0..13 {
            assert_eq!(s.model.get("audio_enable"), 0);
            assert_eq!(s.model.get("config_valid"), 0);
            s.idle(1);
        }
        assert_eq!(s.model.get("config_valid"), 0);
        assert_eq!(s.model.get("config_response_ready"), 0);
        s.idle(1);
        assert_eq!(s.model.get("audio_enable"), 1);
        wait_reply(s);
        assert_eq!(s.model.get("response_code"), 0);
        assert_eq!(data(s), [0; 12]);
        for _ in 0..40 {
            assert_eq!(s.model.get("audio_enable"), 1);
            assert_eq!(s.model.get("config_valid"), 0);
            assert_eq!(access(s, 1, 0, 20), 1);
            assert_eq!(access(s, 2, 0, 20), 6);
        }
        pop(s, 19);
        for (enabled, code) in [(1, 0), (1, 0), (0, 0), (0, 0), (1, 2), (0, 0)] {
            fill(s, &request(9, enabled, 0, [0; 5]));
            assert_eq!(access(s, 1, 0, 21), 0);
            wait_reply(s);
            assert_eq!(s.model.get("response_code"), code);
            assert_eq!(data(s), [0; 12]);
            assert_eq!(s.model.get("config_valid"), 0);
            assert_eq!(
                s.model.get("audio_enable"),
                u64::from(enabled == 1 && code == 0)
            );
            pop(s, 21);
        }
        reset(s);
        fill(s, &words);
        assert_eq!(access(s, 1, 0, 22), 0);
        wait_reply(s);
        assert_eq!(s.model.get("response_code"), 0);
        assert_eq!(s.model.get("audio_enable"), 1);
    });
}

#[shoop_test]
fn malformed_enable_packets_and_halt_cannot_gain_authority_or_replay_local_effects() {
    run(|s| {
        let mut cases = vec![];
        for bit in 1..32 {
            cases.push(request(9, 1 << bit, 0, [0; 5]));
        }
        for index in 3..14 {
            for bit in 0..32 {
                let mut words = request(9, 1, 0, [0; 5]);
                words[index] = 1 << bit;
                cases.push(words);
            }
        }
        for words in cases {
            fill(s, &words);
            assert_eq!(access(s, 1, 0, 23), 0);
            wait_reply(s);
            assert_eq!(s.model.get("response_code"), 2);
            assert_eq!(data(s), [0; 12]);
            assert_eq!(s.model.get("audio_enable"), 0);
            assert_eq!(s.model.get("config_valid"), 0);
            pop(s, 23);
        }
        for position in 0..20 {
            reset(s);
            fill(s, &request(9, 1, 0, [0; 5]));
            assert_eq!(access(s, 1, 0, 24), 0);
            s.idle(position);
            let took_effect = s.model.get("audio_enable") != 0;
            s.model.set("halt", 1);
            s.model.eval();
            assert_eq!(s.model.get("audio_enable"), 0, "immediate halt mask");
            wait_reply(s);
            assert_eq!(
                s.model.get("response_code"),
                if took_effect { 0 } else { 3 }
            );
            assert_eq!(data(s), [0; 12]);
            for _ in 0..25 {
                s.model
                    .set_all(&[("config_ready", 1), ("config_response_valid", 1)]);
                s.idle(1);
                assert_eq!(s.model.get("config_valid"), 0);
                assert_eq!(s.model.get("audio_enable"), 0);
                assert_eq!(access(s, 2, 0, 24), 3);
            }
            s.model.set("live", 0);
            s.idle(2);
            s.model
                .set_all(&[("halt", 0), ("live", 1), ("config_response_valid", 0)]);
            s.idle(3);
            assert_eq!(s.model.get("audio_enable"), 0);
            assert_eq!(s.model.get("response_pending"), 0);
            assert_eq!(s.model.get("request_space"), 1);
            s.idle(30);
            assert_eq!(s.model.get("audio_enable"), 0, "no late replay");
        }
    });
}
