//! Actual canonical ramps and metadata error order for the supported application signal operations.
use super::image_control::ok;
use crate::{Result, bits::Bits, model::Design, sim::Sim};
use rt_fpga_adapter::{
    audio_image::{Dimensions, Transfer},
    image::{Instruction, Ramp, Value},
    image_wire, protocol, resident,
    resident_parameters::Command,
    resident_runtime::{self as wire, Parameter},
};
use rt_runtime::{Epoch, NodeId};
use shoop_rt_engine::{EngineOp, NodeConfiguration, external::SignalKind};
fn control(s: &mut Sim<'_>, t: Transfer) -> (u64, Bits) {
    s.model.set_all(&[
        ("control_valid", 1),
        ("control_opcode", u64::from(t.opcode)),
    ]);
    s.model.set_bits("control_data", &Bits::from(t.words));
    let (before, _) = s.tick(&["control_ready"]);
    assert_eq!(before.get("control_ready"), 1);
    s.model.set("control_valid", 0);
    s.until(20000, "runtime configuration", |m| {
        m.get("control_done") != 0
    });
    let result = (s.model.get("control_error"), s.model.bits("control_result"));
    s.idle(1);
    assert_eq!(s.model.get("runtime_fault"), 0);
    result
}
fn configured(s: &mut Sim<'_>, t: Transfer) -> Bits {
    let (e, d) = control(s, t);
    assert_eq!(e, 0);
    d
}
fn command(s: &mut Sim<'_>, t: Transfer) -> Bits {
    s.model.set("runtime_valid", 1);
    s.model
        .set_bits("runtime_data", &Bits(t.words[..16].to_vec()));
    let (before, _) = s.tick(&["runtime_ready"]);
    assert_eq!(before.get("runtime_ready"), 1);
    s.model.set("runtime_valid", 0);
    s.until(20000, "resident signal result", |m| {
        m.get("runtime_done") != 0
    });
    let result = s.model.bits("runtime_result");
    s.idle(1);
    assert_eq!(s.model.get("runtime_fault"), 0);
    result
}
fn done(s: &mut Sim<'_>, t: Transfer) {
    assert_eq!(command(s, t).get(504, 8), u64::from(resident::DONE));
}
fn refused(s: &mut Sim<'_>, t: Transfer, code: u64) {
    let d = command(s, t);
    assert_eq!((d.get(504, 8), d.get(0, 8)), (1, code));
}
fn activate(s: &mut Sim<'_>, initial: bool, epoch: u64) {
    s.model.set_all(&[
        ("activation_valid", 1),
        ("activation_initial", u64::from(initial)),
        ("activation_stereo", 0),
        ("activation_epoch", epoch),
    ]);
    let (before, _) = s.tick(&["activation_ready"]);
    assert_eq!(before.get("activation_ready"), 1);
    s.model.set("activation_valid", 0);
    s.until(20000, "runtime activation metadata", |m| {
        m.get("activation_done") != 0
    });
    s.idle(1);
    assert_eq!(s.model.get("runtime_fault"), 0);
}
fn observe(s: &mut Sim<'_>, recording: bool, pre: bool) {
    let mut state = [0; 26];
    protocol::put(&mut state, 0, 1, 1);
    protocol::put(&mut state, 33, 2, if recording { 2 } else { 0 });
    s.model.set_bits("observe_data", &Bits::from(state));
    s.model
        .set_all(&[("observe_valid", 1), ("pre_recording", u64::from(pre))]);
    let (before, _) = s.tick(&["observe_ready"]);
    assert_eq!(before.get("observe_ready"), 1);
    s.model.set("observe_valid", 0);
    s.until(20000, "autonomous monitoring", |m| {
        m.get("observe_done") != 0
    });
    s.idle(1);
    assert_eq!(s.model.get("runtime_fault"), 0);
}
fn read(s: &mut Sim<'_>, id: NodeId, p: Parameter) -> Bits {
    configured(
        s,
        Command {
            opcode: 7,
            key: wire::key(id, p).unwrap(),
            ..Default::default()
        }
        .transfer(),
    )
}
pub fn signals() -> Result<()> {
    let compiled = design().build(&crate::root().join("work/marlin-suite/runtime-control"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10000, 0)]);
    s.model.set_all(&[("reset", 1), ("job_ready", 1)]);
    s.idle(2);
    s.model.set("reset", 0);
    s.idle(1);
    let id = |slot| NodeId::from_parts(slot, 1).unwrap();
    let kinds = [
        SignalKind::Playback {
            slot: 0,
            channels: 2,
            auxiliary: 0,
        },
        SignalKind::Capture {
            slot: 0,
            channels: 2,
            auxiliary: 0,
        },
        SignalKind::StripInput {
            slot: 0,
            channels: 2,
            ramp: 64,
        },
        SignalKind::StripOutput {
            slot: 0,
            channels: 2,
            ramp: 128,
        },
        SignalKind::Fader {
            slot: 0,
            channels: 2,
            ramp: 64,
        },
        SignalKind::BusInput {
            slot: 0,
            channels: 2,
        },
        SignalKind::Bus {
            slot: 0,
            channels: 2,
            ramp: 256,
        },
        SignalKind::Reader,
    ];
    for (i, kind) in kinds.into_iter().enumerate() {
        configured(&mut s, wire::metadata(id(i as u32), kind)?);
        for p in [
            Parameter::Gain,
            Parameter::Balance,
            Parameter::Mute,
            Parameter::Presence,
            Parameter::Bank,
            Parameter::Monitor,
            Parameter::Stereo,
        ] {
            let value = match p {
                Parameter::Gain | Parameter::Mute | Parameter::Presence => 1 << 24,
                _ => 0,
            };
            configured(
                &mut s,
                Command::seed(
                    wire::key(id(i as u32), p)?,
                    Ramp {
                        start: value,
                        target: value,
                        duration: 0,
                        elapsed: 0,
                    },
                )
                .transfer(),
            );
        }
    }
    refused(&mut s, wire::processor(id(8), EngineOp::Gain(f32::NAN)), 1);
    refused(&mut s, wire::processor(id(0), EngineOp::Gain(f32::NAN)), 2);
    refused(&mut s, wire::processor(id(0), EngineOp::Gain(0.5)), 12);
    refused(&mut s, wire::processor(id(4), EngineOp::Gain(0.5)), 2);
    done(&mut s, wire::processor(id(4), EngineOp::Presence(false)));
    assert_eq!(read(&mut s, id(4), Parameter::Presence).get(32, 32), 0);
    let config = |token, generation, epoch, inputs, gain| NodeConfiguration {
        processor: None,
        recorded_state: None,
        generation,
        token,
        epoch: Epoch(epoch),
        inputs,
        outputs: 2,
        bank: 1,
        send_bank: 0,
        return_bank: 0,
        gain,
        balance: 0.5,
        monitoring: true,
        muted: false,
    };
    let invalid = config(0, 7, 0, 1, 0.25);
    refused(
        &mut s,
        wire::processor(id(2), EngineOp::Configure(invalid)),
        10,
    );
    let invalid = config(0, 0, 0, 1, 0.25);
    refused(
        &mut s,
        wire::processor(id(2), EngineOp::Configure(invalid)),
        2,
    );
    let valid = config(41, 7, 1, 1, 0.25);
    done(&mut s, wire::processor(id(2), EngineOp::Configure(valid)));
    refused(
        &mut s,
        wire::processor(id(2), EngineOp::Configure(config(42, 7, 1, 1, 0.25))),
        4,
    );
    refused(
        &mut s,
        wire::processor(id(2), EngineOp::DiscardConfiguration { token: 42 }),
        1,
    );
    done(
        &mut s,
        wire::processor(id(3), EngineOp::Configure(config(31, 7, 1, 2, 0.5))),
    );
    done(
        &mut s,
        wire::processor(id(3), EngineOp::DiscardConfiguration { token: 31 }),
    );
    configured(&mut s, wire::policy(id(2), 7, true));
    activate(&mut s, true, 1);
    assert_eq!(read(&mut s, id(2), Parameter::Gain).get(32, 32), 1 << 22);
    assert_eq!(read(&mut s, id(2), Parameter::Balance).get(32, 32), 1 << 23);
    assert_eq!(read(&mut s, id(2), Parameter::Stereo).get(32, 32), 0);
    assert_eq!(
        read(&mut s, id(3), Parameter::Gain).get(32, 32),
        1 << 23,
        "discarding processor pending state must retain canonical configuration"
    );
    assert_eq!(read(&mut s, id(3), Parameter::Stereo).get(32, 32), 1 << 24);
    assert_eq!(read(&mut s, id(0), Parameter::Stereo).get(32, 32), 0);
    assert_eq!(
        read(&mut s, id(2), Parameter::Monitor).get(32, 32),
        0,
        "initial configuration must not replace monitor ramp"
    );
    let nonce = ok(
        &mut s,
        Transfer::begin_bank(
            0,
            0,
            Dimensions {
                instructions: 1,
                states: 4,
                jobs: 1,
                buffers: 2,
                copies: 0,
            },
        ),
    )
    .get(0, 64);
    let instruction = image_wire::instruction_words(Instruction::ProcessGain {
        ramp: 0,
        source: 0,
        destination: 1,
    })?;
    ok(&mut s, Transfer::upload(false, 0, instruction, nonce));
    for address in 0..4 {
        ok(
            &mut s,
            Transfer::upload(
                true,
                address,
                image_wire::value_words(Value::Ramp(Ramp {
                    start: 0,
                    target: 0,
                    duration: 0,
                    elapsed: 0,
                })),
                nonce,
            ),
        );
    }
    configured(&mut s, Command::begin(nonce, 4).transfer());
    for (index, p) in [
        Parameter::Gain,
        Parameter::Gain,
        Parameter::Monitor,
        Parameter::Bank,
    ]
    .into_iter()
    .enumerate()
    {
        configured(
            &mut s,
            Command::binding(
                nonce,
                index as u32,
                wire::key(if index == 3 { id(3) } else { id(2) }, p)?,
                index as u16,
            )
            .transfer(),
        );
    }
    configured(
        &mut s,
        Command {
            opcode: 5,
            nonce,
            bank: false,
            ..Default::default()
        }
        .transfer(),
    );
    ok(&mut s, Transfer::activate_canonical(nonce));
    configured(
        &mut s,
        Command {
            opcode: 6,
            nonce,
            bank: false,
            ..Default::default()
        }
        .transfer(),
    );
    s.model.set("loaded", 1);
    s.model.set("graph_epoch", 1);
    refused(
        &mut s,
        wire::processor(id(2), EngineOp::Configure(invalid)),
        4,
    );
    refused(
        &mut s,
        wire::processor(id(8), EngineOp::Configure(valid)),
        1,
    );
    observe(&mut s, false, false);
    assert_eq!(read(&mut s, id(2), Parameter::Monitor).get(32, 32), 1 << 24);
    let advanced = Ramp {
        start: 0,
        target: 1 << 24,
        duration: 64,
        elapsed: 32,
    };
    ok(
        &mut s,
        Transfer::write(true, 2, image_wire::value_words(Value::Ramp(advanced))),
    );
    observe(&mut s, false, false);
    assert_eq!(
        read(&mut s, id(2), Parameter::Monitor).get(96, 32),
        32,
        "unchanged monitoring restarted ramp"
    );
    done(&mut s, wire::processor(id(2), EngineOp::Monitor(false)));
    observe(&mut s, false, false);
    assert_eq!(read(&mut s, id(2), Parameter::Monitor).get(32, 32), 0);
    observe(&mut s, false, true);
    assert_eq!(read(&mut s, id(2), Parameter::Monitor).get(32, 32), 1 << 24);
    configured(&mut s, wire::policy(id(2), 8, true));
    observe(&mut s, true, false);
    assert_eq!(
        read(&mut s, id(2), Parameter::Monitor).get(32, 32),
        0,
        "foreign configuration generation enabled monitoring"
    );
    done(&mut s, wire::processor(id(2), EngineOp::Gain(-1.)));
    let gain = read(&mut s, id(2), Parameter::Gain);
    assert_eq!(gain.get(0, 32), 1 << 22);
    assert_eq!(gain.get(32, 32) as u32 as i32, -(1 << 24));
    assert_eq!(
        ok(&mut s, Transfer::read(true, 0)),
        ok(&mut s, Transfer::read(true, 1))
    );
    s.model.set("run_valid", 1);
    s.idle(1);
    s.model.set("run_valid", 0);
    s.until(10000, "actual state ramp advancement", |m| {
        m.get("done") != 0
    });
    s.idle(4);
    done(&mut s, wire::processor(id(2), EngineOp::Gain(0.)));
    assert_eq!(
        read(&mut s, id(2), Parameter::Gain).get(0, 32) as u32 as i32,
        -6291456,
        "retime used cached rather than actual elapsed state"
    );
    done(&mut s, wire::processor(id(3), EngineOp::Crossfade));
    assert_eq!(read(&mut s, id(3), Parameter::Bank).get(32, 32), 0);
    done(&mut s, wire::processor(id(7), EngineOp::ReadStop));
    refused(&mut s, wire::processor(id(2), EngineOp::ReadStop), 2);
    assert_eq!(s.model.get("fault"), 0);
    assert_eq!(s.model.get("error_code"), 0);
    println!(
        "runtime signals: metadata identity/error precedence, configured canonical values, discard lifetime, initial stereo shape, actual-ramp retiming/duplicates, monitor generation/manual/pre-recording and unchanged-target continuity"
    );
    Ok(())
}

fn design() -> Design {
    let root = crate::root();
    let mut sources = super::image_control::design().sources;
    sources.extend([
        root.join("fpga/audio_io/tests/parameters_probe.sv"),
        root.join("fpga/audio_io/tests/runtime_probe.sv"),
        root.join("fpga/audio_io/rtl/audio_parameters.sv"),
        root.join("fpga/audio_io/rtl/audio_runtime.sv"),
    ]);
    Design::new("runtime_probe", sources)
}
pub fn conditional_balance() -> Result<()> {
    let compiled = design().build(&crate::root().join("work/marlin-suite/runtime-control"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10000, 0)]);
    s.model.set_all(&[("reset", 1), ("job_ready", 1)]);
    s.idle(2);
    s.model.set("reset", 0);
    s.idle(1);
    configured(
        &mut s,
        Command::seed(
            1,
            Ramp {
                start: 0,
                target: 1 << 24,
                duration: 128,
                elapsed: 0,
            },
        )
        .transfer(),
    );
    configured(
        &mut s,
        Command::seed(
            6,
            Ramp {
                start: 0,
                target: 0,
                duration: 0,
                elapsed: 0,
            },
        )
        .transfer(),
    );
    let constant = |v| Ramp {
        start: v,
        target: v,
        duration: 0,
        elapsed: 0,
    };
    let stereo = |s: &mut Sim<'_>, on| {
        configured(
            s,
            Command {
                opcode: 11,
                key: 6,
                value: Some(Value::Ramp(constant(if on { 1 << 24 } else { 0 }))),
                ..Default::default()
            }
            .transfer(),
        );
    };
    let image = |s: &mut Sim<'_>, bank: u16, version| {
        let nonce = ok(
            s,
            Transfer::begin_bank(
                bank,
                version,
                Dimensions {
                    instructions: 1,
                    states: 1,
                    jobs: 1,
                    buffers: 2,
                    copies: 0,
                },
            ),
        )
        .get(0, 64);
        ok(
            s,
            Transfer::upload(
                false,
                0,
                image_wire::instruction_words(Instruction::ProcessGain {
                    ramp: 0,
                    source: 0,
                    destination: 1,
                })
                .unwrap(),
                nonce,
            ),
        );
        ok(
            s,
            Transfer::upload(
                true,
                0,
                image_wire::value_words(Value::Ramp(constant(99))),
                nonce,
            ),
        );
        configured(s, Command::begin(nonce, 1).transfer());
        configured(
            s,
            Command::conditional_balance(nonce, 0, 1, 0, 0, 1).transfer(),
        );
        nonce
    };
    let swap = |s: &mut Sim<'_>, nonce| {
        configured(
            s,
            Command {
                opcode: 4,
                ..Default::default()
            }
            .transfer(),
        );
        configured(
            s,
            Command {
                opcode: 5,
                nonce,
                bank: nonce & 1 != 0,
                ..Default::default()
            }
            .transfer(),
        );
        ok(s, Transfer::activate_canonical(nonce));
        configured(
            s,
            Command {
                opcode: 6,
                nonce,
                bank: nonce & 1 != 0,
                ..Default::default()
            }
            .transfer(),
        );
    };
    let read = |s: &mut Sim<'_>| {
        configured(
            s,
            Command {
                opcode: 7,
                key: 1,
                ..Default::default()
            }
            .transfer(),
        )
    };
    let run = |s: &mut Sim<'_>| {
        s.model.set("run_valid", 1);
        s.idle(1);
        s.model.set("run_valid", 0);
        s.until(10000, "conditional state execution", |m| m.get("done") != 0);
        s.idle(4);
        assert_eq!(s.model.get("error_code"), 0);
    };
    let nonce = image(&mut s, 0, 0);
    swap(&mut s, nonce);
    let program = ok(&mut s, Transfer::read(false, 0));
    let expected = image_wire::instruction_words(Instruction::ProcessConstant {
        value: 0,
        destination: 1,
    })?;
    assert_eq!(program.array::<5>(), expected);
    for _ in 0..4 {
        run(&mut s);
    }
    assert_eq!(
        read(&mut s).get(96, 32),
        0,
        "disabled balance elapsed advanced"
    );
    configured(
        &mut s,
        Command {
            opcode: 3,
            key: 1,
            target: -(1 << 24),
            duration: 64,
            ..Default::default()
        }
        .transfer(),
    );
    let off = read(&mut s);
    assert_eq!(off.get(0, 32), 0);
    for _ in 0..3 {
        run(&mut s);
    }
    assert_eq!(read(&mut s), off);
    let nonce = image(&mut s, 1, 1);
    stereo(&mut s, true);
    swap(&mut s, nonce);
    assert_eq!(read(&mut s), off);
    run(&mut s);
    assert_eq!(read(&mut s).get(96, 32), 32);
    configured(
        &mut s,
        Command {
            opcode: 3,
            key: 1,
            target: 1 << 24,
            duration: 64,
            ..Default::default()
        }
        .transfer(),
    );
    let on = read(&mut s);
    assert_eq!(on.get(0, 32) as u32 as i32, -(1 << 23));
    run(&mut s);
    let actual = read(&mut s);
    assert_eq!(actual.get(96, 32), 32);
    let nonce = image(&mut s, 0, 2);
    stereo(&mut s, false);
    swap(&mut s, nonce);
    for _ in 0..4 {
        run(&mut s);
    }
    assert_eq!(
        read(&mut s),
        actual,
        "inactive binding was retained as an active head"
    );
    let nonce = image(&mut s, 1, 3);
    stereo(&mut s, true);
    swap(&mut s, nonce);
    assert_eq!(read(&mut s), actual);
    run(&mut s);
    assert_eq!(read(&mut s).get(96, 32), 64);
    assert_eq!(s.model.get("fault"), 0);
    assert_eq!(s.model.get("runtime_fault"), 0);
    println!(
        "conditional balance templates: disabled jobs become zero constants, dropped bindings freeze and accept commands, activation restores original jobs and exact live/frozen progress"
    );
    Ok(())
}
