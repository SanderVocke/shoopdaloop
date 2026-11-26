#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_rtl::{model::Design, sim::Sim};
use shoop_wasm_test_support::shoop_test;

fn run(test: impl FnOnce(&mut Sim<'_>)) {
    let root = rt_fpga_rtl::root();
    let design = Design::new(
        "fifo_group_banks",
        vec![root.join("fpga/host_bridge/rtl/fifo_group_banks.sv")],
    )
    .build(&root.join("work/cmd-fifo-banks"))
    .unwrap();
    let mut sim = Sim::new(design.session(None).unwrap(), &[("clk", 10_000, 0)]);
    sim.max_ps = 100_000_000_000;
    reset(&mut sim, 1);
    test(&mut sim);
}
fn reset(sim: &mut Sim<'_>, epoch: u64) {
    sim.model.set_all(&[
        ("reset", 1),
        ("epoch", epoch),
        ("fault", 0),
        ("host_valid", 0),
        ("take", 0),
        ("command_read_valid", 0),
        ("result_read_valid", 0),
        ("result_write_valid", 0),
        ("finish_valid", 0),
        ("fence_valid", 0),
    ]);
    sim.idle(2);
    sim.model.set("reset", 0);
    sim.idle(1);
}
fn request(
    sim: &mut Sim<'_>,
    operation: u64,
    bank: u64,
    epoch: u64,
    word: u32,
    sequence: u64,
    strobe: u64,
) -> u64 {
    sim.model.set_all(&[
        ("host_valid", 1),
        ("host_operation", operation),
        ("host_bank", bank),
        ("host_epoch", epoch),
        ("host_word", u64::from(word)),
        ("host_sequence", sequence),
        ("host_strobe", strobe),
    ]);
    sim.idle(1);
    assert_eq!(sim.model.get("host_response_valid"), 1);
    let receipt = sim.model.get("host_receipt");
    sim.model.set_all(&[("host_valid", 0), ("take", 0)]);
    sim.idle(1);
    receipt
}
fn host(sim: &mut Sim<'_>, operation: u64, bank: u64, word: u32, sequence: u64) -> u64 {
    let epoch = sim.model.get("epoch");
    request(sim, operation, bank, epoch, word, sequence, 15)
}
fn packet(epoch: u32, sequence: u64, count: usize) -> Vec<u32> {
    let mut words = vec![0; 16 + count * 12];
    words[0] = 0x53434631;
    words[1] = 16 | ((count as u32) << 16);
    words[2] = epoch;
    words[4] = sequence as u32;
    words[5] = (sequence >> 32) as u32;
    words[6] = 2;
    words[8] = 3;
    for (index, value) in words.iter_mut().enumerate().skip(16) {
        *value = 0x12340000 | index as u32;
    }
    words
}
fn fill(sim: &mut Sim<'_>, bank: u64, words: &[u32]) {
    assert_eq!(host(sim, 0, bank, 0, 0), 0);
    for &word in words {
        assert_eq!(host(sim, 1, bank, word, 0), 0);
    }
}
fn publish(sim: &mut Sim<'_>, bank: u64, sequence: u64, count: usize) {
    fill(
        sim,
        bank,
        &packet(sim.model.get("epoch") as u32, sequence, count),
    );
    assert_eq!(host(sim, 2, bank, 0, 0), 0);
}
fn state(sim: &mut Sim<'_>, bank: u64) -> u64 {
    sim.model.set("inspect_bank", bank);
    sim.idle(1);
    sim.model.get("bank_state")
}
fn take(sim: &mut Sim<'_>) -> (u64, u64) {
    assert_eq!(sim.model.get("head_valid"), 1);
    let value = (sim.model.get("head_bank"), sim.model.get("head_sequence"));
    sim.model.set("take", 1);
    sim.idle(1);
    sim.model.set("take", 0);
    sim.idle(1);
    assert_eq!(state(sim, value.0), 3);
    assert_eq!(sim.model.get("head_valid"), 0);
    value
}
fn read(sim: &mut Sim<'_>, result: bool, bank: u64, word: u64) -> (u64, u64) {
    let prefix = if result { "result" } else { "command" };
    sim.model.set(&format!("{prefix}_read_valid"), 1);
    sim.model.set(&format!("{prefix}_read_bank"), bank);
    sim.model.set(&format!("{prefix}_read_word"), word);
    sim.idle(1);
    assert_eq!(sim.model.get(&format!("{prefix}_response_valid")), 1);
    let value = (
        sim.model.get(&format!("{prefix}_response_error")),
        sim.model.get(&format!("{prefix}_read_data")),
    );
    sim.model.set(&format!("{prefix}_read_valid"), 0);
    sim.idle(1);
    value
}
fn write_result(sim: &mut Sim<'_>, bank: u64, word: u64, data: u32) {
    sim.model.set_all(&[
        ("result_write_valid", 1),
        ("result_write_bank", bank),
        ("result_write_word", word),
        ("result_write_data", u64::from(data)),
    ]);
    sim.idle(1);
    sim.model.set("result_write_valid", 0);
    assert_eq!(sim.model.get("protocol_fault"), 0);
}
fn codes(sim: &mut Sim<'_>, bank: u64, count: usize) {
    for index in 0..count {
        write_result(sim, bank, 16 + index as u64, 1);
    }
}
fn header(sim: &mut Sim<'_>, bank: u64, sequence: u64, count: usize) {
    let mut words = [0; 16];
    words[0] = 0x53465231;
    words[1] = 16 | ((count as u32) << 16);
    words[2] = sim.model.get("epoch") as u32;
    words[4] = sequence as u32;
    words[5] = (sequence >> 32) as u32;
    words[12] = 1;
    for (index, word) in words.into_iter().enumerate() {
        write_result(sim, bank, index as u64, word);
    }
}
fn finish(sim: &mut Sim<'_>, bank: u64, sequence: u64, wait: bool) {
    let epoch = sim.model.get("epoch");
    sim.model.set_all(&[
        ("finish_valid", 1),
        ("finish_bank", bank),
        ("finish_epoch", epoch),
        ("finish_sequence", sequence),
        ("finish_wait_fence", u64::from(wait)),
        ("finish_fault", 0),
    ]);
    sim.idle(1);
    sim.model.set("finish_valid", 0);
    sim.idle(1);
}
fn fence(sim: &mut Sim<'_>, bank: u64, epoch: u64, sequence: u64) {
    sim.model.set_all(&[
        ("fence_valid", 1),
        ("fence_bank", bank),
        ("fence_epoch", epoch),
        ("fence_sequence", sequence),
        ("fence_fault", 0),
    ]);
    sim.idle(1);
    sim.model.set("fence_valid", 0);
    sim.idle(1);
}

#[shoop_test]
fn complete_publication_hides_every_partial_word_and_freezes_admitted_payload() {
    run(|sim| {
        assert_eq!(host(sim, 0, 3, 0, 0), 0);
        assert_eq!(request(sim, 1, 3, 1, 0x53434631, 0, 3), 7);
        assert_eq!(state(sim, 3), 1);
        assert_eq!(sim.model.get("written_words"), 0);
        let words = packet(1, 1, 32);
        for &word in &words {
            assert_eq!(host(sim, 1, 3, word, 0), 0);
            assert_eq!(sim.model.get("head_valid"), 0);
            sim.idle(3);
        }
        assert_eq!(read(sim, false, 3, 0).0, 1);
        assert_eq!(host(sim, 2, 3, 0, 0), 0);
        assert_eq!(sim.model.get("last_admitted"), 1);
        assert_eq!(sim.model.get("next_sequence"), 2);
        assert_eq!(host(sim, 1, 3, u32::MAX, 0), 5);
        assert_eq!(host(sim, 0, 3, 0, 0), 1);
        assert_eq!(host(sim, 2, 3, 0, 0), 6);
        assert_eq!(take(sim), (3, 1));
        for index in [0, 5, 15, 16, 399] {
            assert_eq!(
                read(sim, false, 3, index),
                (0, u64::from(words[index as usize]))
            );
        }
        assert_eq!(read(sim, false, 3, 400).0, 1);
        codes(sim, 3, 32);
        header(sim, 3, 1, 32);
        finish(sim, 3, 1, false);
        assert_eq!(sim.model.get("result_ready_mask"), 8);
        assert_eq!(read(sim, true, 3, 0), (0, 0x53465231));
        assert_eq!(read(sim, true, 3, 47), (0, 1));
        assert_eq!(read(sim, true, 3, 48).0, 1);
        assert_eq!(request(sim, 3, 3, 2, 0, 1, 15), 2);
        assert_eq!(host(sim, 3, 3, 0, 1), 0);
        assert_eq!(sim.model.get("free_mask"), 15);
    });
}

#[shoop_test]
fn four_reserved_results_limit_admission_without_stopping_later_execution() {
    run(|sim| {
        let order = [2, 0, 3, 1];
        for (index, bank) in order.into_iter().enumerate() {
            publish(sim, bank, index as u64 + 1, 1);
        }
        for (index, bank) in order.into_iter().enumerate() {
            let sequence = index as u64 + 1;
            assert_eq!(take(sim), (bank, sequence));
            codes(sim, bank, 1);
            header(sim, bank, sequence, 1);
            finish(sim, bank, sequence, false);
            assert_eq!(sim.model.get("result_ready_mask"), 4);
        }
        assert_eq!(sim.model.get("free_mask"), 0);
        assert_eq!(host(sim, 0, 2, 0, 0), 1);
        assert_eq!(host(sim, 3, 0, 0, 2), 5);
        assert_eq!(read(sim, true, 0, 0).0, 1);
        assert_eq!(host(sim, 3, 2, 0, 1), 0);
        publish(sim, 2, 5, 1);
        assert_eq!(host(sim, 3, 2, 0, 1), 0);
        assert_eq!(state(sim, 2), 2);
        assert_eq!(take(sim), (2, 5));
        codes(sim, 2, 1);
        header(sim, 2, 5, 1);
        finish(sim, 2, 5, false);
        assert_eq!(sim.model.get("result_ready_mask"), 1);
        assert_eq!(sim.model.get("free_mask"), 0);
        assert_eq!(sim.model.get("protocol_fault"), 0);
    });
}

#[shoop_test]
fn delayed_fence_preserves_result_order_and_epoch_sequence_ownership() {
    run(|sim| {
        publish(sim, 1, 1, 1);
        assert_eq!(take(sim), (1, 1));
        codes(sim, 1, 1);
        finish(sim, 1, 1, true);
        assert_eq!(state(sim, 1), 4);
        publish(sim, 0, 2, 1);
        assert_eq!(take(sim), (0, 2));
        codes(sim, 0, 1);
        header(sim, 0, 2, 1);
        finish(sim, 0, 2, false);
        assert_eq!(sim.model.get("result_ready_mask"), 0);
        assert_eq!(read(sim, true, 0, 0).0, 1);
        assert_eq!(host(sim, 3, 0, 0, 2), 5);
        fence(sim, 1, 2, 1);
        fence(sim, 1, 1, 2);
        assert_eq!(state(sim, 1), 4);
        header(sim, 1, 1, 1);
        fence(sim, 1, 1, 1);
        assert_eq!(sim.model.get("result_ready_mask"), 2);
        assert_eq!(host(sim, 3, 1, 0, 1), 0);
        assert_eq!(sim.model.get("result_ready_mask"), 1);
        reset(sim, 2);
        assert_eq!(sim.model.get("free_mask"), 15);
        assert_eq!(sim.model.get("next_sequence"), 1);
        assert_eq!(sim.model.get("result_ready_mask"), 0);
        publish(sim, 1, 1, 1);
        assert_eq!(take(sim), (1, 1));
        codes(sim, 1, 1);
        finish(sim, 1, 1, true);
        fence(sim, 1, 1, 1);
        assert_eq!(state(sim, 1), 4);
        assert_eq!(request(sim, 3, 1, 1, 0, 1, 15), 2);
        header(sim, 1, 1, 1);
        fence(sim, 1, 2, 1);
        assert_eq!(sim.model.get("result_ready_mask"), 2);
        assert_eq!(sim.model.get("protocol_fault"), 0);
    });
}

#[shoop_test]
fn malformed_incomplete_and_same_edge_publication_never_execute_partial_work() {
    run(|sim| {
        for (index, value, expected) in [
            (0, 0, 4),
            (1, 15 | (1 << 16), 4),
            (2, 2, 2),
            (3, 2, 4),
            (10, 7, 4),
            (12, 1, 4),
        ] {
            let mut words = packet(1, 1, 1);
            words[index] = value;
            fill(sim, 0, &words);
            assert_eq!(host(sim, 2, 0, 0, 0), expected);
            assert_eq!(sim.model.get("head_valid"), 0);
            assert_eq!(sim.model.get("next_sequence"), 1);
        }
        fill(sim, 0, &packet(1, 1, 1)[..16]);
        assert_eq!(host(sim, 2, 0, 0, 0), 7);
        fill(sim, 0, &packet(1, 2, 1));
        assert_eq!(host(sim, 2, 0, 0, 0), 4);
        fill(sim, 0, &packet(1, 1, 1));
        sim.model.set("take", 1);
        assert_eq!(host(sim, 2, 0, 0, 0), 0);
        assert_eq!(state(sim, 0), 2);
        assert_eq!(take(sim), (0, 1));
        finish(sim, 0, 1, false);
        assert_eq!(state(sim, 0), 6);
        assert_eq!(sim.model.get("protocol_fault"), 1);
        assert_eq!(sim.model.get("result_ready_mask"), 0);
        assert_eq!(host(sim, 0, 1, 0, 0), 3);
    });
}
