//! Complete batches, autonomous empty/partial ingress, bounded arbitration and failures.
use crate::{
    Result,
    bits::Bits,
    drivers::stream::{Source, control_packet},
    model::Design,
    sim::Sim,
};
fn header(sequence: u64, commands: u32) -> Bits {
    Bits(
        rt_fpga_adapter::audio_control::header(sequence, sequence + 1, commands, 5)
            .unwrap()
            .to_vec(),
    )
}
pub fn batches_and_failures() -> Result<()> {
    let root = crate::root();
    let compiled = Design::new(
        "audio_control",
        vec![
            root.join("fpga/state_processor/rtl/sp_ram.sv"),
            root.join("fpga/audio_io/rtl/audio_command_fifo.sv"),
            root.join("fpga/audio_io/rtl/audio_control.sv"),
        ],
    )
    .parameters(&[("DEPTH", 5), ("MAX_BATCH_CYCLES", 64)])
    .build(&root.join("work/marlin-suite/control"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
    let reset = |s: &mut Sim<'_>| {
        s.model.set("reset", 1);
        s.idle(1);
        s.model
            .set_all(&[("reset", 0), ("enable", 1), ("result_ready", 1)]);
        s.idle(1);
    };
    reset(&mut s);
    s.model.set_all(&[("result_ready", 0), ("epoch", 7)]);
    let source = Source::attach(
        &mut s,
        "control_valid",
        "control_ready",
        "control_data",
        vec![],
    );
    s.agent(crate::protocol::ReadyValid::new(
        "command_valid",
        "command_ready",
        &["command_data", "command_opcode", "command_index"],
    ));
    s.agent(crate::protocol::ReadyValid::new(
        "cfg_valid",
        "cfg_ready",
        &["cfg_state", "cfg_address", "cfg_write_data"],
    ));
    let advance = |s: &mut Sim<'_>, sequence: u64| {
        assert_eq!(s.model.get("run_permitted"), 1);
        s.model.set("run_start", 1);
        s.idle(1);
        s.model
            .set_all(&[("run_start", 0), ("sequence_number", sequence)]);
    };
    for sequence in 1..=1024 {
        advance(&mut s, sequence);
        assert_eq!(s.model.get("command_valid") | s.model.get("fault"), 0);
    }
    source.borrow_mut().queue.extend([
        header(1024, 2),
        control_packet(1024, 0, &Bits::from_u64(123), 4, 0),
    ]);
    s.idle(20);
    assert_eq!(
        s.model.get("command_valid"),
        0,
        "partial batch became visible"
    );
    for sequence in 1025..=2048 {
        advance(&mut s, sequence);
    }
    source.borrow_mut().queue.extend([
        control_packet(1024, 1, &Bits::from_u64(456), 0, (1 << 31) | 7),
        header(2048, 1),
        control_packet(2048, 0, &Bits::from_u64(789), 5, 1),
    ]);
    s.model.set("admission_blocked", 1);
    s.idle(20);
    assert_eq!(s.model.get("command_valid"), 0);
    assert_eq!(
        s.model.get("run_permitted"),
        1,
        "held host reply gated audio"
    );
    s.model.set("admission_blocked", 0);
    s.until(1000, "late complete batch dispatch", |m| {
        m.get("command_valid") != 0
    });
    assert_eq!(s.model.get("run_permitted"), 0);
    for _ in 0..3 {
        assert_eq!(s.model.bits("command_data").get(0, 64), 123);
        assert_eq!(s.model.get("command_opcode"), 4);
        s.idle(1);
    }
    s.model.set("command_ready", 1);
    s.idle(1);
    s.model.set("command_ready", 0);
    assert_eq!(s.model.get("response_ready"), 1);
    s.model.set("response_valid", 1);
    s.model.set_bits("response_data", &Bits::from_u64(0xfeed));
    s.idle(1);
    s.model.set("response_valid", 0);
    s.until(1000, "configuration dispatch", |m| m.get("cfg_valid") != 0);
    assert_eq!(s.model.get("cfg_state"), 1);
    assert_eq!(s.model.get("cfg_address"), 7);
    assert_eq!(s.model.bits("cfg_write_data").get(0, 64), 456);
    assert_eq!(s.model.get("run_permitted"), 0, "batch was interrupted");
    s.model.set("cfg_ready", 1);
    s.idle(1);
    s.model
        .set_all(&[("cfg_ready", 0), ("cfg_response_valid", 1)]);
    s.idle(1);
    s.model.set("cfg_response_valid", 0);
    s.idle(10);
    assert_eq!(
        s.model.get("command_valid"),
        0,
        "flood exceeded one batch per quantum"
    );
    advance(&mut s, 2049);
    s.until(1000, "next batch", |m| m.get("command_valid") != 0);
    assert_eq!(s.model.bits("command_data").get(0, 64), 789);
    s.model.set("command_ready", 1);
    s.idle(1);
    s.model
        .set_all(&[("command_ready", 0), ("response_valid", 1)]);
    s.idle(1);
    s.model.set("response_valid", 0);
    s.idle(5);
    assert_eq!(s.model.get("fault"), 0);
    assert!(source.borrow().queue.is_empty());
    let first = s.model.bits("result_data");
    assert_eq!(s.model.get("result_valid"), 1);
    assert_eq!(first.get(878, 64), 1025);
    assert_eq!(first.get(942, 64), 2048);
    assert_eq!(first.get(1006, 32), 7);
    assert_eq!(first.get(846, 32), 0);
    assert_eq!(first.get(838, 8), 4);
    let decoded =
        rt_fpga_adapter::audio_control::Completion::decode(&first.0.clone().try_into().unwrap())?;
    assert_eq!(decoded.batch, 1025);
    assert_eq!(decoded.quantum, 2048);
    assert_eq!(decoded.epoch, 7);
    assert_eq!(decoded.payload[0], 0xfeed);
    source.borrow_mut().queue.extend([
        header(2049, 3),
        control_packet(2049, 0, &Bits::default(), 0, 0),
        control_packet(2049, 0, &Bits::default(), 0, 0),
        control_packet(2049, 0, &Bits::default(), 0, 0),
    ]);
    advance(&mut s, 2050);
    s.idle(20);
    assert_eq!(
        s.model.get("command_valid"),
        0,
        "batch started without all reply credits"
    );
    assert_eq!(s.model.get("run_permitted"), 1);
    assert_eq!(s.model.bits("result_data"), first);
    s.model.set("result_ready", 1);
    s.idle(1);
    s.model.set("result_ready", 0);
    for _ in 0..3 {
        s.until(1000, "reserved reply batch", |m| {
            m.get("command_valid") != 0
        });
        s.model.set("command_ready", 1);
        s.idle(1);
        s.model
            .set_all(&[("command_ready", 0), ("response_valid", 1)]);
        s.idle(1);
        s.model.set("response_valid", 0);
    }
    s.idle(10);
    source.borrow_mut().queue.extend([
        header(2050, 1),
        control_packet(2050, 0, &Bits::default(), 4, 0),
    ]);
    advance(&mut s, 2051);
    s.idle(15);
    let held = s.model.bits("result_data");
    for sequence in 2052..3076 {
        advance(&mut s, sequence);
        assert_eq!(s.model.get("command_valid"), 0, "full replies gated audio");
        assert_eq!(s.model.bits("result_data"), held);
    }
    s.model.set("result_ready", 1);
    s.until(1000, "admission after reply release", |m| {
        m.get("command_valid") != 0
    });
    s.model.set("command_ready", 1);
    s.idle(1);
    s.model.set_all(&[
        ("command_ready", 0),
        ("response_valid", 1),
        ("response_error", 3),
    ]);
    s.model.set_bits("response_data", &Bits::default());
    s.idle(1);
    s.model.set_all(&[
        ("response_valid", 0),
        ("response_error", 0),
        ("result_ready", 0),
    ]);
    s.idle(10);
    assert_eq!(
        s.model.get("fault"),
        0,
        "ordinary command refusal faulted audio"
    );
    assert_eq!(s.model.get("run_permitted"), 1);
    s.model.set("result_ready", 1);
    s.until(1000, "retained refusal", |m| {
        m.get("result_valid") != 0 && m.bits("result_data").get(878, 64) == 2051
    });
    s.model.set("result_ready", 0);
    assert_eq!(s.model.bits("result_data").get(832, 4), 3);

    reset(&mut s);
    s.model.set("sequence_number", 0);
    source
        .borrow_mut()
        .queue
        .extend([header(10, 1), control_packet(10, 0, &Bits::default(), 4, 0)]);
    s.idle(15);
    for sequence in 1..=10 {
        assert_eq!(s.model.get("command_valid"), 0, "future batch ran early");
        advance(&mut s, sequence);
    }
    s.until(1000, "future batch", |m| m.get("command_valid") != 0);
    s.until(1000, "bounded device timeout", |m| m.get("fault") != 0);
    assert_eq!(s.model.get("run_permitted"), 0);

    for malformed in [
        control_packet(0, 2, &Bits::default(), 0, 0),
        control_packet(0, 2, &Bits::default(), 0xa1, 5),
        control_packet(0, 2, &Bits::default(), 0xa1, 0),
    ] {
        reset(&mut s);
        source.borrow_mut().queue.push_back(malformed);
        s.until(1000, "malformed batch rejection", |m| m.get("fault") != 0);
        assert_eq!(s.model.get("command_valid") | s.model.get("cfg_valid"), 0);
    }
    reset(&mut s);
    source
        .borrow_mut()
        .queue
        .extend([header(0, 1), control_packet(0, 0, &Bits::default(), 16, 0)]);
    s.until(1000, "forbidden settlement", |m| m.get("fault") != 0);
    assert_eq!(s.model.get("command_valid"), 0);
    Ok(())
}
