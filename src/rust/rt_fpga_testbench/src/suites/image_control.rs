//! Staging isolation and activation-time state against the real state processor.
use crate::{
    Result,
    bits::Bits,
    clock::Edges,
    model::{Design, Model, Probe},
    sim::{Agent, Sim},
};
use rt_fpga_adapter::{
    audio_image::{Dimensions, Transfer},
    image::{Instruction, Ramp, Value},
};
use std::{cell::RefCell, rc::Rc};
struct Jobs(Rc<RefCell<Vec<Bits>>>);
impl Agent for Jobs {
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if e.rises(0) && m.get("job_valid") != 0 && m.get("job_ready") != 0 {
            self.0.borrow_mut().push(m.bits("job_data"));
        }
    }
}
fn request(s: &mut Sim<'_>, t: Transfer) -> (u64, Bits) {
    s.model.set_all(&[
        ("request_valid", 1),
        ("request_opcode", u64::from(t.opcode)),
        ("request_state", u64::from(t.state)),
        ("request_address", u64::from(t.address)),
    ]);
    s.model.set_bits("request_data", &Bits(t.words.to_vec()));
    let mut accepted = false;
    for _ in 0..1000 {
        let (before, _) = s.tick(&["request_ready"]);
        if before.get("request_ready") != 0 {
            accepted = true;
            break;
        }
    }
    assert!(accepted, "image request admission timed out");
    s.model.set("request_valid", 0);
    s.until(20000, "image response", |m| m.get("response_valid") != 0);
    let result = (s.model.get("response_error"), s.model.bits("response_data"));
    s.idle(1);
    result
}
fn ok(s: &mut Sim<'_>, t: Transfer) -> Bits {
    let op = t.opcode;
    let address = t.address;
    let (error, data) = request(s, t);
    assert_eq!(error, 0, "configuration opcode {op} address {address}");
    data
}
fn run(s: &mut Sim<'_>, jobs: &Rc<RefCell<Vec<Bits>>>) -> Bits {
    s.model.set("run_valid", 1);
    let (before, _) = s.tick(&["run_ready"]);
    assert_ne!(before.get("run_ready"), 0);
    s.model.set("run_valid", 0);
    s.until(10000, "state processor execution", |m| m.get("done") != 0);
    assert_eq!(s.model.get("error_code"), 0);
    s.idle(4);
    let mut rows = jobs.borrow_mut();
    assert_eq!(rows.len(), 1);
    rows.pop().unwrap()
}
fn design() -> Design {
    let root = crate::root();
    Design::new(
        "image_probe",
        vec![
            root.join("fpga/audio_io/tests/image_probe.sv"),
            root.join("fpga/audio_io/rtl/audio_image.sv"),
            root.join("fpga/state_processor/rtl/sp_ram.sv"),
            root.join("fpga/state_processor/rtl/sp_job_fifo.sv"),
            root.join("fpga/state_processor/rtl/sp_engine.sv"),
            root.join("fpga/state_processor/rtl/state_processor.sv"),
            root.join("fpga/job_processor/rtl/jp_divider.sv"),
        ],
    )
}
pub fn staging_and_ramps() -> Result<()> {
    let compiled = design()
        .probes(vec![
            Probe::inject("image.nonce"),
            Probe::inject("image.version"),
        ])
        .build(&crate::root().join("work/marlin-suite/image-control"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10000, 0)]);
    s.model.set_all(&[("reset", 1), ("job_ready", 1)]);
    s.idle(2);
    s.model.set("reset", 0);
    s.idle(1);
    let jobs = Rc::new(RefCell::new(Vec::new()));
    s.agent(Jobs(jobs.clone()));
    let instruction = |ramp| {
        rt_fpga_sim::instruction_words(Instruction::ProcessGain {
            ramp,
            source: 0,
            destination: 1,
        })
        .unwrap()
    };
    let ramp = Ramp {
        start: -(1 << 24),
        target: 1 << 24,
        duration: 128,
        elapsed: 0,
    };
    ok(&mut s, Transfer::write(false, 0, instruction(0)));
    ok(
        &mut s,
        Transfer::write(true, 0, rt_fpga_sim::value_words(Value::Ramp(ramp))),
    );
    ok(
        &mut s,
        Transfer::write(true, 1, rt_fpga_sim::value_words(Value::Word(99))),
    );
    assert_eq!(run(&mut s, &jobs).get(160, 32), 0);
    let nonce = ok(
        &mut s,
        Transfer::begin(
            1,
            Dimensions {
                instructions: 1,
                states: 2,
                jobs: 1,
                buffers: 2,
                copies: 1,
            },
        ),
    )
    .get(0, 64);
    assert_eq!(nonce, 1);
    ok(&mut s, Transfer::upload(false, 0, instruction(1), nonce));
    assert_eq!(
        request(&mut s, Transfer::activate(nonce)).0,
        5,
        "partial image published"
    );
    assert_eq!(
        run(&mut s, &jobs).get(160, 32),
        32,
        "staging changed active state"
    );
    ok(
        &mut s,
        Transfer::upload(true, 0, rt_fpga_sim::value_words(Value::Word(123)), nonce),
    );
    ok(
        &mut s,
        Transfer::upload(
            true,
            1,
            rt_fpga_sim::value_words(Value::Ramp(Ramp { elapsed: 0, ..ramp })),
            nonce,
        ),
    );
    assert_eq!(
        request(&mut s, Transfer::copy(0, 0, 1, true, nonce + 1)).0,
        3
    );
    ok(&mut s, Transfer::copy(0, 0, 1, true, nonce));
    assert_eq!(ok(&mut s, Transfer::activate(nonce)).get(0, 64), 2);
    assert_eq!(
        request(&mut s, Transfer::activate(nonce)).0,
        3,
        "activation replay was accepted"
    );
    assert_eq!(
        run(&mut s, &jobs).get(160, 32),
        64,
        "activation published an old ramp snapshot"
    );
    let changed = ok(&mut s, Transfer::set_ramp(1, -(1 << 24), 64, 2));
    assert_eq!(changed.get(0, 32) as i32, 1 << 23);
    assert_eq!(changed.get(32, 32) as i32, -(1 << 24));
    assert_eq!(changed.get(64, 32), 64);
    assert_eq!(changed.get(96, 32), 0);
    for old in [
        Ramp {
            start: i32::MIN,
            target: i32::MAX,
            duration: u32::MAX,
            elapsed: u32::MAX - 1,
        },
        Ramp {
            start: i32::MAX,
            target: i32::MIN,
            duration: u32::MAX,
            elapsed: u32::MAX - 1,
        },
        Ramp {
            start: i32::MIN,
            target: i32::MAX,
            duration: 0,
            elapsed: u32::MAX,
        },
        Ramp {
            start: i32::MAX,
            target: i32::MIN,
            duration: 123,
            elapsed: 999,
        },
        Ramp {
            start: 12345,
            target: -67890,
            duration: 97,
            elapsed: 31,
        },
    ] {
        ok(
            &mut s,
            Transfer::write(true, 1, rt_fpga_sim::value_words(Value::Ramp(old))),
        );
        let expected = rt_audio_math::Ramp {
            start: rt_runtime::Q824(old.start),
            target: rt_runtime::Q824(old.target),
            duration: old.duration,
            elapsed: old.elapsed,
        }
        .at(0)
        .0;
        let actual = ok(&mut s, Transfer::set_ramp(1, 17, 42, 2));
        assert_eq!(
            actual.get(0, 32) as i32,
            expected,
            "full-range ramp interpolation"
        );
    }
    assert_eq!(
        request(&mut s, Transfer::set_ramp(1, 0, 0, 1)).0,
        3,
        "stale active version"
    );
    let previous = ok(&mut s, Transfer::read(true, 1));
    let token = ok(
        &mut s,
        Transfer::begin(
            2,
            Dimensions {
                instructions: 1,
                states: 2,
                jobs: 1,
                buffers: 2,
                copies: 1,
            },
        ),
    )
    .get(0, 64);
    ok(&mut s, Transfer::upload(false, 0, instruction(1), token));
    for address in 0..2 {
        ok(
            &mut s,
            Transfer::upload(
                true,
                address,
                rt_fpga_sim::value_words(Value::Word(7)),
                token,
            ),
        );
    }
    ok(&mut s, Transfer::copy(0, 1, 0, true, token));
    assert_eq!(
        request(&mut s, Transfer::activate(token)).0,
        5,
        "copy type mismatch was published"
    );
    assert_eq!(
        ok(&mut s, Transfer::read(true, 1)),
        previous,
        "refused image changed active state"
    );
    assert_eq!(s.model.get("fault"), 0);
    s.model
        .inject("image.nonce", Some(&Bits::from_u64(u64::MAX)));
    s.idle(1);
    assert_eq!(
        request(
            &mut s,
            Transfer::begin(
                2,
                Dimensions {
                    instructions: 1,
                    states: 2,
                    jobs: 1,
                    buffers: 2,
                    copies: 0
                }
            )
        )
        .0,
        8
    );
    s.model.inject("image.nonce", None);
    s.model
        .inject("image.version", Some(&Bits::from_u64(u64::MAX)));
    s.idle(1);
    assert_eq!(
        request(&mut s, Transfer::write(false, 0, instruction(0))).0,
        8
    );
    s.model.inject("image.version", None);
    s.model.set("reset", 1);
    s.idle(2);
    s.model.set("reset", 0);
    s.idle(1);
    assert_eq!(ok(&mut s, Transfer::status()).get(0, 64), 0);
    Ok(())
}

pub fn full_capacity_activation() -> Result<()> {
    let compiled = design()
        .parameters(&[
            ("PROGRAM_DEPTH", 512),
            ("STATE_DEPTH", 512),
            ("JOB_DEPTH", 512),
            ("INITIAL_STATES", 512),
        ])
        .build(&crate::root().join("work/marlin-suite/image-control"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10000, 0)]);
    s.model.set_all(&[("reset", 1), ("job_ready", 1)]);
    s.idle(2);
    s.model.set("reset", 0);
    s.idle(1);
    let program = rt_fpga_sim::instruction_words(Instruction::ProcessConstant {
        value: 17,
        destination: 0,
    })?;
    ok(&mut s, Transfer::write(false, 0, program));
    for address in 0..512 {
        ok(
            &mut s,
            Transfer::write(
                true,
                address,
                rt_fpga_sim::value_words(Value::Word(0x123400000000 + u64::from(address))),
            ),
        );
    }
    let nonce = ok(
        &mut s,
        Transfer::begin(
            1,
            Dimensions {
                instructions: 512,
                states: 512,
                jobs: 512,
                buffers: 2,
                copies: 512,
            },
        ),
    )
    .get(0, 64);
    for address in 0..512 {
        ok(&mut s, Transfer::upload(false, address, program, nonce));
    }
    for address in 0..512 {
        ok(
            &mut s,
            Transfer::upload(
                true,
                address,
                rt_fpga_sim::value_words(Value::Word(0)),
                nonce,
            ),
        );
    }
    for index in 0..512 {
        ok(
            &mut s,
            Transfer::copy(index, index, 511 - index, false, nonce),
        );
    }
    let before = s.cycles;
    ok(&mut s, Transfer::activate(nonce));
    let elapsed = s.cycles - before;
    assert!(
        elapsed <= 6 * 512 + 4 * (512 + 512) + 32,
        "activation exceeded its conservative work bound: {elapsed}"
    );
    for address in 0..512 {
        assert_eq!(
            ok(&mut s, Transfer::read(true, address)).get(0, 64),
            0x123400000000 + u64::from(511 - address)
        );
    }
    assert_eq!(request(&mut s, Transfer::read(true, u16::MAX)).0, 1);
    assert_eq!(s.model.get("fault"), 0);
    Ok(())
}

fn action_design() -> Design {
    let root = crate::root();
    let mut sources = design().sources;
    sources.extend([
        root.join("fpga/audio_io/tests/actions_probe.sv"),
        root.join("fpga/audio_io/rtl/audio_actions.sv"),
    ]);
    Design::new("actions_probe", sources).probes(vec![
        Probe::inject("actions.nonce"),
        Probe::inject("actions.version"),
    ])
}
pub fn resident_configuration_actions() -> Result<()> {
    use rt_fpga_adapter::audio_actions::{self as a, Identity, Record};
    let compiled = action_design().build(&crate::root().join("work/marlin-suite/image-control"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10000, 0)]);
    s.model.set_all(&[("reset", 1), ("job_ready", 1)]);
    s.idle(2);
    s.model.set("reset", 0);
    s.idle(8);
    let jobs = Rc::new(RefCell::new(Vec::new()));
    s.agent(Jobs(jobs.clone()));
    let program = rt_fpga_sim::instruction_words(Instruction::ProcessGain {
        ramp: 0,
        source: 0,
        destination: 1,
    })?;
    let mut expected = rt_audio_math::Ramp {
        start: rt_runtime::Q824(0),
        target: rt_runtime::Q824(1 << 24),
        duration: 1024,
        elapsed: 0,
    };
    ok(&mut s, Transfer::write(false, 0, program));
    ok(
        &mut s,
        Transfer::write(
            true,
            0,
            rt_fpga_sim::value_words(Value::Ramp(Ramp {
                start: 0,
                target: 1 << 24,
                duration: 1024,
                elapsed: 0,
            })),
        ),
    );
    let nonce = ok(
        &mut s,
        Transfer::begin(
            1,
            Dimensions {
                instructions: 1,
                states: 1,
                jobs: 1,
                buffers: 2,
                copies: 1,
            },
        ),
    )
    .get(0, 64);
    ok(&mut s, Transfer::upload(false, 0, program, nonce));
    ok(
        &mut s,
        Transfer::upload(
            true,
            0,
            rt_fpga_sim::value_words(Value::Ramp(Ramp {
                start: 0,
                target: 0,
                duration: 0,
                elapsed: 0,
            })),
            nonce,
        ),
    );
    ok(&mut s, Transfer::copy(0, 0, 0, true, nonce));
    let activation = Identity {
        slot: 3,
        generation: 1,
    };
    let parameter = Identity {
        slot: 0,
        generation: 1,
    };
    ok(
        &mut s,
        a::install(activation, 10, 63, &Transfer::activate(nonce))?,
    );
    ok(
        &mut s,
        a::install(
            parameter,
            20,
            96,
            &Transfer::set_ramp(0, -(1 << 24), 128, 2),
        )?,
    );
    let stage = ok(&mut s, a::begin(0, 2)).get(0, 64);
    ok(&mut s, a::node(0, parameter, stage));
    assert_eq!(
        request(&mut s, a::commit(stage)).0,
        5,
        "partial membership became visible"
    );
    ok(&mut s, a::node(1, activation, stage));
    assert_eq!(
        request(&mut s, a::commit(stage)).0,
        5,
        "unsorted membership became visible"
    );
    let stage = ok(&mut s, a::begin(0, 2)).get(0, 64);
    ok(&mut s, a::node(0, activation, stage));
    ok(&mut s, a::node(1, parameter, stage));
    assert_eq!(ok(&mut s, a::commit(stage)).get(0, 64), 1);
    // A far-future resident action leaves the immediate configuration port usable.
    let far = Identity {
        slot: 1,
        generation: 1,
    };
    ok(
        &mut s,
        a::install(far, 30, 1 << 20, &Transfer::set_ramp(0, 0, 0, 2))?,
    );
    let stage = ok(&mut s, a::begin(1, 3)).get(0, 64);
    for (i, id) in [activation, parameter, far].into_iter().enumerate() {
        ok(&mut s, a::node(i as u16, id, stage));
    }
    ok(&mut s, a::commit(stage));
    assert_eq!(ok(&mut s, Transfer::status()).get(0, 64), 1);
    // No command, observation, completion or membership service during these quanta.
    for quantum in 0..1050 {
        let frame = quantum * 32;
        s.model.set("current_frame", frame);
        s.model.set("cut_start", 1);
        s.idle(1);
        s.model.set("cut_start", 0);
        s.until(16384, "resident configuration cut", |m| {
            m.get("cut_done") != 0
        });
        assert_eq!(s.model.get("request_valid"), 0);
        assert_eq!(s.model.get("action_fault"), 0);
        if frame == 96 {
            expected = rt_audio_math::Ramp {
                start: expected.at(0),
                target: rt_runtime::Q824(-(1 << 24)),
                duration: 128,
                elapsed: 0,
            };
        }
        let job = run(&mut s, &jobs);
        assert_eq!(
            job.get(64, 32) as i32,
            expected.start.0,
            "ramp start at frame {frame}"
        );
        assert_eq!(
            job.get(96, 32) as i32,
            expected.target.0,
            "ramp target at frame {frame}"
        );
        assert_eq!(job.get(128, 32), u64::from(expected.duration));
        assert_eq!(job.get(160, 32), u64::from(expected.elapsed));
        expected.elapsed = expected.elapsed.saturating_add(32).min(expected.duration);
    }
    for (id, frame) in [(activation, 64), (parameter, 96)] {
        let raw = ok(&mut s, a::read(id));
        let record = Record::decode(id.slot, &raw.0.try_into().unwrap())?;
        assert!(record.terminal && record.published);
        assert_eq!(record.error, 0);
        assert_eq!(record.terminal_frame, frame);
        assert_eq!(record.effective_frame, frame);
        ok(&mut s, a::collect(id));
        assert_eq!(
            request(&mut s, a::read(id)).0,
            3,
            "collected generation was readable"
        );
        assert_eq!(
            request(
                &mut s,
                a::install(id, 40, 40000, &Transfer::set_ramp(0, 0, 0, 2))?
            )
            .0,
            3,
            "stale identity reinstalled"
        );
    }
    let current = s.model.get("current_frame");
    let recycled = Identity {
        slot: 0,
        generation: 2,
    };
    ok(
        &mut s,
        a::install(recycled, 40, 0, &Transfer::set_ramp(0, 17, 0, 2))?,
    );
    let stage = ok(&mut s, a::begin(2, 2)).get(0, 64);
    ok(&mut s, a::node(0, recycled, stage));
    ok(&mut s, a::node(1, far, stage));
    let result = ok(&mut s, a::commit(stage));
    assert_eq!(result.get(64, 64), current);
    let raw = ok(&mut s, a::read(recycled));
    let record = Record::decode(0, &raw.0.try_into().unwrap())?;
    assert_eq!(
        record.effective_frame, current,
        "late publication did not use the actual device cut"
    );
    s.model.set("cut_start", 1);
    s.idle(1);
    s.model.set("cut_start", 0);
    s.until(16384, "recycled resident row", |m| m.get("cut_done") != 0);
    let raw = ok(&mut s, a::read(recycled));
    let record = Record::decode(0, &raw.0.try_into().unwrap())?;
    assert!(record.terminal);
    assert_eq!(record.error, 0);
    assert_eq!(
        request(
            &mut s,
            a::install(
                Identity {
                    slot: 2,
                    generation: 1
                },
                50,
                u64::MAX,
                &Transfer::set_ramp(0, 0, 0, 2)
            )?
        )
        .0,
        8
    );
    s.model
        .inject("actions.nonce", Some(&Bits::from_u64(u64::MAX)));
    s.idle(1);
    assert_eq!(request(&mut s, a::begin(3, 0)).0, 8);
    s.model.inject("actions.nonce", None);
    s.model.set("reset", 1);
    s.idle(2);
    s.model.set("reset", 0);
    s.idle(8);
    s.model
        .inject("actions.version", Some(&Bits::from_u64(u64::MAX)));
    s.idle(1);
    let stage = ok(&mut s, a::begin(u64::MAX, 0)).get(0, 64);
    assert_eq!(request(&mut s, a::commit(stage)).0, 8);
    s.model.inject("actions.version", None);
    assert_eq!(s.model.get("action_fault"), 0);
    Ok(())
}

pub fn configuration_owner_and_budget() -> Result<()> {
    use rt_fpga_adapter::audio_actions::{Identity, Progress, Service};
    fn drive(s: &mut Sim<'_>, owner: &mut Service, mut transfer: Transfer) -> Progress {
        for _ in 0..100 {
            let (error, words) = request(s, transfer);
            match owner
                .finish(
                    error as u8,
                    words.0.try_into().unwrap(),
                    s.model.get("current_frame"),
                )
                .unwrap()
            {
                Progress::Transfer(next) => transfer = next,
                result => return result,
            }
        }
        panic!("configuration owner did not finish its bounded publication");
    }
    let compiled = action_design().build(&crate::root().join("work/marlin-suite/image-control"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10000, 0)]);
    s.model.set_all(&[("reset", 1), ("job_ready", 1)]);
    s.idle(2);
    s.model.set("reset", 0);
    s.idle(8);
    let program = rt_fpga_sim::instruction_words(Instruction::ProcessGain {
        ramp: 0,
        source: 0,
        destination: 1,
    })?;
    ok(&mut s, Transfer::write(false, 0, program));
    ok(
        &mut s,
        Transfer::write(
            true,
            0,
            rt_fpga_sim::value_words(Value::Ramp(Ramp {
                start: 0,
                target: 0,
                duration: 0,
                elapsed: 0,
            })),
        ),
    );
    let mut owner = Service::new(4, 32)?;
    let action = |target| Transfer::set_ramp(0, target, 64, 1);
    for sequence in 1..=4 {
        let t = owner
            .schedule(sequence, 31, action(sequence as i32))
            .unwrap();
        match drive(&mut s, &mut owner, t) {
            Progress::Admitted {
                identity,
                effective_frame,
            } => {
                assert_eq!(
                    identity,
                    Identity {
                        slot: sequence as u16 - 1,
                        generation: 1
                    }
                );
                assert_eq!(effective_frame, 32);
            }
            _ => panic!("configuration was not admitted"),
        }
    }
    assert!(
        matches!(owner.schedule(1, 31, action(0)), Err(4)),
        "duplicate identity was not retained"
    );
    assert!(
        matches!(owner.schedule(5, 31, action(0)), Err(2)),
        "shared configuration pool was not bounded"
    );
    s.model.set_all(&[("current_frame", 32), ("cut_start", 1)]);
    let before = s.cycles;
    s.idle(1);
    s.model.set("cut_start", 0);
    s.until(512, "bounded resident configuration work", |m| {
        m.get("cut_done") != 0
    });
    assert!(s.cycles - before <= 256 + 6 * 4 + 32);
    assert_eq!(s.model.get("action_fault"), 0);
    for slot in 0..4 {
        let t = owner
            .inspect(
                Identity {
                    slot,
                    generation: 1,
                },
                false,
            )
            .unwrap();
        match drive(&mut s, &mut owner, t) {
            Progress::Record { record, .. } => {
                assert!(record.terminal);
                assert_eq!(record.terminal_frame, 32);
                assert_eq!(record.error, if slot < 2 { 0 } else { 2 });
            }
            _ => panic!("missing retained configuration result"),
        }
    }
    assert!(
        matches!(owner.schedule(1, 31, action(0)), Err(4)),
        "terminal duplicate released before collection"
    );
    let id = Identity {
        slot: 1,
        generation: 1,
    };
    let t = owner.inspect(id, true).unwrap();
    assert!(matches!(
        drive(&mut s, &mut owner, t),
        Progress::Record { .. }
    ));
    let t = owner.schedule(5, 64, action(5)).unwrap();
    assert!(matches!(
        drive(&mut s, &mut owner, t),
        Progress::Admitted {
            identity: Identity {
                slot: 1,
                generation: 2
            },
            effective_frame: 64
        }
    ));
    let t = owner
        .cancel(Identity {
            slot: 1,
            generation: 2,
        })
        .unwrap();
    match drive(&mut s, &mut owner, t) {
        Progress::Record { record, .. } => {
            assert!(record.terminal);
            assert_eq!(record.error, 11);
        }
        _ => panic!("cancellation did not retain its result"),
    }
    assert!(matches!(owner.schedule(5, 64, action(5)), Err(4)));
    let t = owner
        .inspect(
            Identity {
                slot: 1,
                generation: 2,
            },
            true,
        )
        .unwrap();
    drive(&mut s, &mut owner, t);
    let _pending = owner.schedule(6, 96, action(6)).unwrap();
    owner.uncertain();
    assert!(
        matches!(owner.schedule(7, 96, action(7)), Err(5)),
        "uncertain ownership was reused"
    );
    s.model.set_all(&[("reset", 1), ("current_frame", 0)]);
    s.idle(2);
    s.model.set("reset", 0);
    s.idle(8);
    ok(&mut s, Transfer::write(false, 0, program));
    ok(
        &mut s,
        Transfer::write(
            true,
            0,
            rt_fpga_sim::value_words(Value::Ramp(Ramp {
                start: 0,
                target: 0,
                duration: 0,
                elapsed: 0,
            })),
        ),
    );
    let mut owner = Service::new(4, 32)?;
    let transfer = owner.schedule(1, 64, action(1)).unwrap();
    drive(&mut s, &mut owner, transfer);
    let mut transfer = owner.schedule(100, 0, action(100)).unwrap();
    let mut crossed = false;
    let mut publications = 0;
    loop {
        if transfer.opcode == 16 {
            publications += 1;
        }
        if transfer.opcode == 18 && !crossed {
            s.model.set("current_frame", 64);
            crossed = true;
        }
        let (error, data) = request(&mut s, transfer);
        match owner.finish(
            error as u8,
            data.0.try_into().unwrap(),
            s.model.get("current_frame"),
        )? {
            Progress::Transfer(next) => transfer = next,
            Progress::Admitted {
                effective_frame, ..
            } => {
                assert_eq!(effective_frame, 64);
                break;
            }
            _ => panic!("moving-cut publication did not retry"),
        }
        assert!(publications <= 2, "publication retry did not converge");
    }
    assert_eq!(
        publications, 2,
        "late clamping did not exercise atomic order refusal/retry"
    );
    s.model.set("cut_start", 1);
    s.idle(1);
    s.model.set("cut_start", 0);
    s.until(512, "clamped configuration order", |m| {
        m.get("cut_done") != 0
    });
    assert_eq!(
        ok(&mut s, Transfer::read(true, 0)).get(32, 32),
        100,
        "same-cut sequence order changed"
    );
    Ok(())
}

pub fn canonical_staging_banks() -> Result<()> {
    let compiled = design()
        .parameters(&[("BANKS", 2), ("CANONICAL", 1)])
        .build(&crate::root().join("work/marlin-suite/canonical-image"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10000, 0)]);
    s.model.set_all(&[("reset", 1), ("job_ready", 1)]);
    s.idle(2);
    s.model.set("reset", 0);
    s.idle(1);
    let jobs = Rc::new(RefCell::new(Vec::new()));
    s.agent(Jobs(jobs.clone()));
    let instruction = rt_fpga_sim::instruction_words(Instruction::ProcessGain {
        ramp: 0,
        source: 0,
        destination: 1,
    })?;
    let initial = Ramp {
        start: 0,
        target: 1 << 24,
        duration: 128,
        elapsed: 0,
    };
    ok(&mut s, Transfer::write(false, 0, instruction));
    ok(
        &mut s,
        Transfer::write(true, 0, rt_fpga_sim::value_words(Value::Ramp(initial))),
    );
    ok(
        &mut s,
        Transfer::write(true, 1, rt_fpga_sim::value_words(Value::Word(99))),
    );
    let dimensions = Dimensions {
        instructions: 1,
        states: 2,
        jobs: 1,
        buffers: 2,
        copies: 0,
    };
    let a = ok(&mut s, Transfer::begin_bank(0, 1, dimensions)).get(0, 64);
    let b = ok(&mut s, Transfer::begin_bank(1, 1, dimensions)).get(0, 64);
    assert_ne!(a, b);
    assert_eq!((a & 1, b & 1), (0, 1));
    for (nonce, value) in [(a, -(1 << 24)), (b, 1 << 24)] {
        ok(&mut s, Transfer::upload(false, 0, instruction, nonce));
        ok(
            &mut s,
            Transfer::upload(
                true,
                0,
                rt_fpga_sim::value_words(Value::Ramp(Ramp {
                    start: value,
                    target: value,
                    duration: 0,
                    elapsed: 0,
                })),
                nonce,
            ),
        );
        ok(
            &mut s,
            Transfer::upload(
                true,
                1,
                rt_fpga_sim::value_words(Value::Word(if nonce == a { 111 } else { 222 })),
                nonce,
            ),
        );
    }
    assert_eq!(
        ok(&mut s, Transfer::read_staged(1, false, a)).get(0, 64),
        111
    );
    assert_eq!(
        ok(&mut s, Transfer::read_staged(1, false, b)).get(0, 64),
        222
    );
    assert_eq!(ok(&mut s, Transfer::read(true, 1)).get(0, 64), 99);
    run(&mut s, &jobs);
    run(&mut s, &jobs);
    let unchanged = ok(&mut s, Transfer::read(true, 0));
    for (duration, elapsed, start, target) in [
        (0, 0, i32::MIN, i32::MAX),
        (1, 0, i32::MIN, i32::MAX),
        (u32::MAX, u32::MAX / 2, i32::MIN, i32::MAX),
        (u32::MAX, u32::MAX / 2, i32::MAX, i32::MIN),
        (128, 127, -1234567, 7654321),
    ] {
        let old = Ramp {
            start,
            target,
            duration,
            elapsed,
        };
        let expected = rt_audio_math::Ramp {
            start: rt_audio_math::Q824(start),
            target: rt_audio_math::Q824(target),
            duration,
            elapsed,
        }
        .at(0)
        .0;
        let before = s.cycles;
        let value = ok(
            &mut s,
            Transfer::evaluate_ramp(rt_fpga_sim::value_words(Value::Ramp(old)), -17, 257),
        );
        assert_eq!(value.get(0, 32) as u32 as i32, expected);
        assert_eq!(value.get(32, 32) as u32 as i32, -17);
        assert_eq!(value.get(64, 32), 257);
        assert_eq!(value.get(96, 32), 0);
        assert_eq!(value.get(128, 1), 1);
        if duration == 0 || elapsed == 0 {
            assert!(s.cycles - before < 16, "zero-progress ramp used division");
        }
    }
    assert_eq!(
        ok(&mut s, Transfer::read(true, 0)),
        unchanged,
        "value-only retiming wrote active state"
    );
    ok(
        &mut s,
        Transfer::patch_staged(1, rt_fpga_sim::value_words(Value::Word(333)), false, a),
    );
    assert_eq!(
        ok(&mut s, Transfer::read_staged(1, false, a)).get(0, 64),
        333
    );
    assert_eq!(
        ok(&mut s, Transfer::read_staged(1, false, b)).get(0, 64),
        222
    );
    assert_eq!(
        request(
            &mut s,
            Transfer::patch_staged(1, rt_fpga_sim::value_words(Value::Ramp(initial)), true, a)
        )
        .0,
        5
    );
    assert_eq!(request(&mut s, Transfer::read_staged(2, false, a)).0, 3);
    assert_eq!(request(&mut s, Transfer::begin_bank(2, 1, dimensions)).0, 3);
    assert_eq!(ok(&mut s, Transfer::activate(b)).get(0, 64), 2);
    assert_eq!(
        request(&mut s, Transfer::activate(a)).0,
        3,
        "ordinary activation accepted stale active addresses"
    );
    let actual = ok(&mut s, Transfer::read(true, 0));
    ok(&mut s, Transfer::patch_staged(0, actual.array(), true, a));
    assert_eq!(ok(&mut s, Transfer::activate_canonical(a)).get(0, 64), 3);
    assert_eq!(ok(&mut s, Transfer::read(true, 0)), actual);
    assert_eq!(ok(&mut s, Transfer::read(true, 1)).get(0, 64), 333);
    assert_eq!(run(&mut s, &jobs).get(96, 32), 1 << 24);
    assert_eq!(request(&mut s, Transfer::read_staged(0, true, b)).0, 3);
    let next = ok(&mut s, Transfer::begin_bank(0, 3, dimensions)).get(0, 64);
    assert_ne!(next, a);
    assert_eq!(request(&mut s, Transfer::read_staged(0, true, a)).0, 3);
    assert_eq!(ok(&mut s, Transfer::status_bank(0)).get(64, 64), next);
    assert_eq!(s.model.get("fault"), 0);
    println!(
        "two inactive images: isolated uploads/nonces, checked private staged patch/read, actual-value retiming with elapsed-zero fast path, stale normal ACT refusal and canonical ACT continuity"
    );
    Ok(())
}

#[derive(Default)]
struct ParameterOp {
    opcode: u8,
    key: u16,
    bank: bool,
    nonce: u64,
    count: u32,
    address: u16,
    value: Option<Ramp>,
    word: Option<u64>,
    target: i32,
    duration: u32,
}
fn parameter(s: &mut Sim<'_>, op: ParameterOp) -> (u64, Bits) {
    s.model.set_all(&[
        ("parameter_valid", 1),
        ("parameter_opcode", u64::from(op.opcode)),
        ("parameter_key", u64::from(op.key)),
        ("parameter_bank", u64::from(op.bank)),
        ("parameter_nonce", op.nonce),
        ("parameter_count", u64::from(op.count)),
        ("parameter_address", u64::from(op.address)),
        ("parameter_target", u64::from(op.target as u32)),
        ("parameter_duration", u64::from(op.duration)),
    ]);
    s.model.set_bits(
        "parameter_value",
        &Bits(
            op.value
                .map(Value::Ramp)
                .or(op.word.map(Value::Word))
                .map_or([0; 5], rt_fpga_sim::value_words)
                .to_vec(),
        ),
    );
    let (before, _) = s.tick(&["parameter_ready"]);
    assert_eq!(before.get("parameter_ready"), 1);
    s.model.set("parameter_valid", 0);
    s.until(20000, "canonical parameter response", |m| {
        m.get("parameter_response_valid") != 0
    });
    let result = (
        s.model.get("parameter_response_error"),
        s.model.bits("parameter_response_data"),
    );
    s.idle(1);
    assert_eq!(s.model.get("parameter_fault"), 0);
    result
}
fn parameter_ok(s: &mut Sim<'_>, op: ParameterOp) -> Bits {
    let (error, data) = parameter(s, op);
    assert_eq!(error, 0);
    data
}
pub fn canonical_parameters() -> Result<()> {
    let mut sources = design().sources;
    sources.push(crate::root().join("fpga/audio_io/rtl/audio_parameters.sv"));
    sources.push(crate::root().join("fpga/audio_io/tests/parameters_probe.sv"));
    let compiled = Design::new("parameters_probe", sources)
        .build(&crate::root().join("work/marlin-suite/canonical-parameters"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10000, 0)]);
    s.model
        .set_all(&[("reset", 1), ("job_ready", 1), ("initialization_open", 1)]);
    s.idle(2);
    s.model.set("reset", 0);
    s.idle(1);
    let jobs = Rc::new(RefCell::new(Vec::new()));
    s.agent(Jobs(jobs.clone()));
    let initial = Ramp {
        start: 0,
        target: 1 << 24,
        duration: 128,
        elapsed: 0,
    };
    let constant = |n| Ramp {
        start: n,
        target: n,
        duration: 0,
        elapsed: 0,
    };
    for (key, value) in [
        (0, initial),
        (1, constant(-(1 << 24))),
        (2, constant(1 << 22)),
    ] {
        parameter_ok(
            &mut s,
            ParameterOp {
                key,
                value: Some(value),
                ..Default::default()
            },
        );
    }
    let instruction = rt_fpga_sim::instruction_words(Instruction::ProcessGain {
        ramp: 0,
        source: 0,
        destination: 1,
    })?;
    let stage = |s: &mut Sim<'_>, bank, version, bindings: &[(u16, u16)]| {
        let nonce = ok(
            s,
            Transfer::begin_bank(
                bank,
                version,
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
        ok(s, Transfer::upload(false, 0, instruction, nonce));
        for address in 0..4 {
            ok(
                s,
                Transfer::upload(
                    true,
                    address,
                    rt_fpga_sim::value_words(if address == 3 {
                        Value::Word(999)
                    } else {
                        Value::Ramp(constant(123))
                    }),
                    nonce,
                ),
            );
        }
        parameter_ok(
            s,
            ParameterOp {
                opcode: 1,
                bank: bank != 0,
                nonce,
                count: bindings.len() as u32 + 1,
                ..Default::default()
            },
        );
        for (index, &(key, address)) in bindings.iter().enumerate() {
            parameter_ok(
                s,
                ParameterOp {
                    opcode: 2,
                    key,
                    address,
                    bank: bank != 0,
                    nonce,
                    count: index as u32,
                    ..Default::default()
                },
            );
        }
        parameter_ok(
            s,
            ParameterOp {
                opcode: 10,
                bank: bank != 0,
                nonce,
                count: bindings.len() as u32,
                address: 3,
                word: Some(if bank == 0 { 123 } else { 456 }),
                ..Default::default()
            },
        );
        nonce
    };
    let activate = |s: &mut Sim<'_>, bank, nonce| {
        parameter_ok(
            s,
            ParameterOp {
                opcode: 5,
                bank,
                nonce,
                ..Default::default()
            },
        );
        ok(s, Transfer::activate_canonical(nonce));
        parameter_ok(
            s,
            ParameterOp {
                opcode: 6,
                bank,
                nonce,
                ..Default::default()
            },
        );
    };
    let read = |s: &mut Sim<'_>, key| {
        parameter_ok(
            s,
            ParameterOp {
                opcode: 7,
                key,
                ..Default::default()
            },
        )
    };
    let first = stage(&mut s, 0, 0, &[(0, 0), (1, 1), (0, 2)]);
    activate(&mut s, false, first);
    assert_eq!(ok(&mut s, Transfer::read(true, 3)).get(0, 64), 123);
    parameter_ok(
        &mut s,
        ParameterOp {
            opcode: 9,
            target: 200,
            word: Some(1),
            ..Default::default()
        },
    );
    assert_eq!(
        ok(&mut s, Transfer::read(true, 3)).get(0, 64),
        123 | (200u64 << 32)
    );
    s.model.set("initialization_open", 0);
    assert_eq!(
        parameter(
            &mut s,
            ParameterOp {
                key: 0,
                value: Some(initial),
                ..Default::default()
            }
        )
        .0,
        5
    );
    run(&mut s, &jobs);
    run(&mut s, &jobs);
    assert_eq!(read(&mut s, 0).get(96, 32), 64);
    let value = parameter_ok(
        &mut s,
        ParameterOp {
            opcode: 3,
            key: 0,
            target: -(1 << 24),
            duration: 64,
            ..Default::default()
        },
    );
    assert_eq!(value.get(0, 32), 1 << 23, "retiming ignored actual elapsed");
    assert_eq!(ok(&mut s, Transfer::read(true, 0)), value);
    assert_eq!(
        ok(&mut s, Transfer::read(true, 2)),
        value,
        "duplicate binding was not updated"
    );
    assert_eq!(read(&mut s, 1).get(32, 32) as u32 as i32, -(1 << 24));
    for _ in 0..3 {
        run(&mut s, &jobs);
    }
    ok(
        &mut s,
        Transfer::write(true, 2, rt_fpga_sim::value_words(Value::Ramp(constant(66)))),
    );
    assert_eq!(
        read(&mut s, 0).get(32, 32) as u32 as i32,
        -(1 << 24),
        "live read must choose first binding"
    );
    let second = stage(&mut s, 1, 1, &[(1, 0)]);
    parameter_ok(
        &mut s,
        ParameterOp {
            opcode: 4,
            ..Default::default()
        },
    );
    activate(&mut s, true, second);
    assert_eq!(
        ok(&mut s, Transfer::read(true, 3)).get(0, 64),
        456 | (200u64 << 32),
        "activation reused stale smoothing or buffer base"
    );
    parameter_ok(
        &mut s,
        ParameterOp {
            opcode: 9,
            target: 0,
            word: Some(1),
            ..Default::default()
        },
    );
    assert_eq!(
        s.model.get("smoothing_linear"),
        1,
        "LinearRestart(0) lost its identity"
    );
    assert_eq!(s.model.get("smoothing_frames"), 0);
    assert_eq!(
        read(&mut s, 0).get(32, 32),
        66,
        "snapshot must choose last binding"
    );
    let frozen = parameter_ok(
        &mut s,
        ParameterOp {
            opcode: 3,
            key: 0,
            target: 1 << 24,
            duration: 32,
            ..Default::default()
        },
    );
    assert_eq!(frozen.get(0, 32), 66);
    for _ in 0..4 {
        run(&mut s, &jobs);
    }
    assert_eq!(read(&mut s, 0), frozen, "dropped key advanced");
    let third = stage(&mut s, 0, 2, &[(0, 0), (2, 1), (0, 2)]);
    assert_ne!(third, first);
    assert_eq!(
        parameter(
            &mut s,
            ParameterOp {
                opcode: 5,
                nonce: first,
                ..Default::default()
            }
        )
        .0,
        3
    );
    parameter_ok(
        &mut s,
        ParameterOp {
            opcode: 4,
            ..Default::default()
        },
    );
    activate(&mut s, false, third);
    assert_eq!(read(&mut s, 0), frozen);
    assert_eq!(ok(&mut s, Transfer::read(true, 2)), frozen);
    assert_eq!(read(&mut s, 2).get(32, 32), 1 << 22);
    let job = run(&mut s, &jobs);
    assert_eq!(job.get(64, 32), 66);
    assert_eq!(job.get(96, 32), 1 << 24);
    assert_eq!(read(&mut s, 0).get(96, 32), 32);
    assert_eq!(
        parameter(
            &mut s,
            ParameterOp {
                opcode: 7,
                key: 8,
                ..Default::default()
            }
        )
        .0,
        3
    );
    assert_eq!(s.model.get("fault"), 0);
    println!(
        "canonical keys: live first-binding reads, full-range retiming, duplicate writes, last-binding snapshots, frozen dropped keys, staged graph continuity and stale nonce guards"
    );
    Ok(())
}
