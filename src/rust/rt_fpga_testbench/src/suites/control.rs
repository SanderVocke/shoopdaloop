//! Quantum fences, bounded ingress, command/configuration responses, and malformed sequences.
use crate::{
    Result,
    bits::Bits,
    drivers::stream::{Source, control_packet},
    model::Design,
    sim::Sim,
};
pub fn fences_and_failures() -> Result<()> {
    let root = crate::root();
    let compiled = Design::new(
        "audio_control",
        vec![
            root.join("fpga/state_processor/rtl/sp_ram.sv"),
            root.join("fpga/audio_io/rtl/audio_command_fifo.sv"),
            root.join("fpga/audio_io/rtl/audio_control.sv"),
        ],
    )
    .parameters(&[("DEPTH", 3)])
    .build(&root.join("work/marlin-suite/control"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
    s.model.set("reset", 1);
    s.idle(1);
    s.model.set("reset", 0);
    let packets = [
        (0, 0, 123, 4, 0),
        (0, 2, 0, 0, 0),
        (1, 1, 456, 0, (1 << 31) | 7),
        (1, 2, 0, 0, 0),
        (3, 0, 789, 0, 0),
    ]
    .into_iter()
    .map(|(seq, kind, data, op, index)| control_packet(seq, kind, &Bits::from_u64(data), op, index))
    .collect();
    let source = Source::attach(
        &mut s,
        "control_valid",
        "control_ready",
        "control_data",
        packets,
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
    s.idle(15);
    assert_eq!(
        s.model.get("control_ready") | s.model.get("command_valid"),
        0
    );
    s.model.set("enable", 1);
    s.until(1000, "command dispatch", |m| m.get("command_valid") != 0);
    for _ in 0..3 {
        assert_eq!(s.model.bits("command_data").get(0, 64), 123);
        assert_eq!(s.model.get("command_opcode"), 4);
        s.idle(1);
    }
    s.model.set("command_ready", 1);
    s.idle(1);
    s.model.set("command_ready", 0);
    assert_eq!(s.model.get("fenced"), 0);
    s.model.set("response_valid", 1);
    s.idle(1);
    s.model.set("response_valid", 0);
    s.until(1000, "first fence", |m| m.get("fenced") != 0);
    for _ in 0..5 {
        s.idle(1);
        assert_eq!(s.model.get("cfg_valid"), 0);
    }
    s.model.set_all(&[("run_start", 1), ("busy", 1)]);
    s.idle(1);
    s.model.set_all(&[("run_start", 0), ("sequence_number", 1)]);
    for _ in 0..5 {
        s.idle(1);
        assert_eq!(s.model.get("cfg_valid"), 0);
    }
    s.model.set("busy", 0);
    s.until(1000, "configuration dispatch", |m| m.get("cfg_valid") != 0);
    assert_eq!(s.model.get("cfg_state"), 1);
    assert_eq!(s.model.get("cfg_address"), 7);
    assert_eq!(s.model.bits("cfg_write_data").get(0, 64), 456);
    s.model.set("cfg_ready", 1);
    s.idle(1);
    s.model
        .set_all(&[("cfg_ready", 0), ("cfg_response_valid", 1)]);
    s.idle(1);
    s.model.set("cfg_response_valid", 0);
    s.until(1000, "second fence", |m| m.get("fenced") != 0);
    s.model.set("run_start", 1);
    s.idle(1);
    s.model.set_all(&[("run_start", 0), ("sequence_number", 2)]);
    for _ in 0..10 {
        s.idle(1);
        assert_eq!(s.model.get("command_valid"), 0);
    }
    assert_ne!(s.model.get("fault"), 0);
    assert!(source.borrow().queue.is_empty());
    Ok(())
}
