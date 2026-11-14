//! Native execution-cell allocation/reuse with the real resolver and no host reply dependency.
use crate::{
    Result,
    bits::Bits,
    drivers::transition::Transition,
    model::{Design, Probe},
    sim::Sim,
};
use rt_fpga_adapter::transition_service::wire;
use serde_json::json;
fn request(s: &mut Sim<'_>, opcode: u64, index: u64, data: &Bits) -> (u64, Bits) {
    s.model.set_all(&[
        ("request_valid", 1),
        ("request_opcode", opcode),
        ("request_index", index),
    ]);
    s.model.set_bits("request_data", data);
    s.until(1000, "resident allocator ready", |m| {
        m.get("request_ready") != 0
    });
    s.idle(1);
    s.model.set("request_valid", 0);
    s.until(2000, "bounded resident allocation", |m| {
        m.get("request_done") != 0
    });
    let result = (s.model.get("request_error"), s.model.bits("request_result"));
    s.idle(1);
    result
}
fn execute(s: &mut Sim<'_>) {
    s.model.set("execute_valid", 1);
    s.until(1000, "resident execute ready", |m| {
        m.get("execute_ready") != 0
    });
    s.idle(1);
    s.model.set("execute_valid", 0);
    s.until(2000, "resident execute done", |m| {
        m.get("execute_done") != 0
    });
    assert_eq!(s.model.get("execute_error"), 0);
    s.idle(1);
}
fn install(
    s: &mut Sim<'_>,
    sequence: u64,
    at: u64,
    operation: serde_json::Value,
    ticks: Option<u64>,
) -> (u64, Bits) {
    let data = super::transition::command(
        0,
        sequence,
        operation,
        at,
        ticks.map(|_| 0),
        ticks.unwrap_or(0),
    );
    let original = crate::drivers::transition::command(&data).array();
    request(s, 4, 0, &Bits::from(wire::install(&original, 0)))
}
pub fn cells_and_initial_activation() -> Result<()> {
    let root = crate::root();
    let compiled = Design::new(
        "native_probe",
        vec![
            root.join("fpga/loop_pipeline/tests/native_probe.sv"),
            root.join("fpga/loop_pipeline/rtl/native_requests.sv"),
            root.join("fpga/transition_processor/rtl/transition_processor.sv"),
            root.join("fpga/transition_processor/rtl/tp_ram.sv"),
            root.join("fpga/transition_processor/rtl/tp_priority.sv"),
        ],
    )
    .probes(vec![Probe::observe("timing.frame")])
    .build(&root.join("work/marlin-suite/native-actions"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10000, 0)]);
    let mut d = Transition::new([1, 4, 6]);
    let case = super::transition::case(
        "native-cells",
        vec![super::transition::spec(0, 64, Some(0), true)],
        vec![],
        16,
        Some(1),
        4,
    );
    let image = rt_backend_testbench::transition_fixture::compile(&serde_json::from_value(
        case["plan"].clone(),
    )?)
    .map_err(|e| format!("{e:?}"))?;
    d.load(&mut s, &serde_json::to_value(image)?);
    d.service = None;
    let held = d.hold_raw_command(&mut s, 0, 0, &Bits::default(), false);
    let stable = |s: &Sim<'_>| {
        assert_eq!(s.model.get("response_valid"), 1);
        assert_eq!(s.model.get("response_error"), held.0);
        assert_eq!(s.model.bits("response_data"), held.1);
    };
    let (error, row) = install(&mut s, 1, 0, super::transition::play(0, true), None);
    assert_eq!(error, 0);
    assert_eq!((row.get(0, 32), row.get(32, 32)), (0, 1));
    assert_eq!(row.get(64 + 205, 64), 0);
    execute(&mut s);
    stable(&s);
    assert_eq!(request(&mut s, 6, 0, &Bits::from_u64(1)).0, 0);
    let (error, row) = install(&mut s, 2, 0, super::transition::seek(0, true), Some(2));
    assert_eq!(error, 0);
    assert_eq!((row.get(0, 32), row.get(32, 32)), (0, 2));
    assert_eq!(row.get(64 + 205, 64), 16);
    let (error, row) = install(&mut s, 10, 256, json!({"kind":"stop"}), None);
    assert_eq!(error, 0);
    assert_eq!(row.get(0, 32), 1);
    for seq in [4, 5] {
        let (error, _) = install(&mut s, seq, 1000, super::transition::play(0, true), None);
        assert_eq!(error, 0);
    }
    assert_eq!(
        install(&mut s, 6, 0, super::transition::play(0, true), None).0,
        2
    );
    while s.model.get("timing.frame") < 208 {
        execute(&mut s);
        stable(&s);
    }
    let (error, tick) = request(&mut s, 13, 0, &Bits::default());
    assert_eq!(error, 0);
    assert_eq!(
        tick.get(37, 3),
        1,
        "generation-updated roster lost tick delivery"
    );
    assert_eq!(tick.get(333, 64), 192);
    assert_eq!(request(&mut s, 6, 0, &Bits::from_u64(2)).0, 0);
    let (error, row) = install(&mut s, 6, 400, super::transition::play(0, true), None);
    assert_eq!(error, 0);
    assert_eq!((row.get(0, 32), row.get(32, 32)), (0, 3));
    while s.model.get("timing.frame") < 288 {
        execute(&mut s);
        stable(&s);
    }
    for slot in [2, 3] {
        let (error, row) = request(&mut s, 13, slot, &Bits::default());
        assert_eq!(error, 0);
        assert_eq!(
            row.get(37, 3),
            3,
            "STOP did not cancel older future native cells"
        );
        assert_eq!(row.get(333, 64), 256);
    }
    assert_eq!(
        request(&mut s, 18, 0, &Bits::default()).0,
        5,
        "fixed roster permitted host replacement"
    );
    stable(&s);
    let empty = super::transition::case("initial-at-cut", vec![], vec![], 16, Some(1), 4);
    let image = rt_backend_testbench::transition_fixture::compile(&serde_json::from_value(
        empty["plan"].clone(),
    )?)
    .map_err(|e| format!("{e:?}"))?;
    d.load(&mut s, &serde_json::to_value(image)?);
    d.service = None;
    for _ in 0..8 {
        execute(&mut s);
    }
    assert_eq!(s.model.get("timing.frame"), 128);
    let spec = crate::drivers::transition::spec(&super::transition::spec(0, 0, Some(0), true));
    assert_eq!(request(&mut s, 22, 0, &spec).0, 0);
    assert_eq!(s.model.get("timing.frame"), 128);
    assert_eq!(request(&mut s, 22, 0, &spec).0, 5);
    let (error, row) = install(&mut s, 1, 0, json!({"kind":"record"}), None);
    assert_eq!(error, 0);
    assert_eq!(row.get(64 + 205, 64), 128);
    execute(&mut s);
    let (error, state) = request(&mut s, 12, 0, &Bits::default());
    assert_eq!(error, 0);
    assert_eq!(state.get(33, 2), 2);
    assert_eq!(state.get(192, 64), 16);
    assert_eq!(s.model.get("timing.frame"), 144);
    println!(
        "native cells: lowest-slot reuse, updated forward generation tags, future/tick/STOP semantics, held host reply and frame-preserving initial activation"
    );
    Ok(())
}

pub fn public_rows_and_private_writeback() -> Result<()> {
    use rt_fpga_adapter::{
        audio_actions::{self as a, Progress, Service},
        audio_image::Transfer,
    };
    let root = crate::root();
    let compiled = Design::new(
        "audio_actions",
        vec![
            root.join("fpga/audio_io/rtl/audio_actions.sv"),
            root.join("fpga/state_processor/rtl/sp_ram.sv"),
        ],
    )
    .parameters(&[("SLOTS", 4), ("NATIVE_ACTIONS", 1)])
    .build(&root.join("work/marlin-suite/public-action-rows"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10000, 0)]);
    s.model.set("reset", 1);
    s.idle(2);
    s.model.set("reset", 0);
    s.idle(8);
    fn exchange(s: &mut Sim<'_>, t: &Transfer) -> (u8, [u32; 26]) {
        s.model.set_all(&[
            ("request_valid", 1),
            ("request_opcode", t.opcode.into()),
            ("request_state", u64::from(t.state)),
            ("request_address", t.address.into()),
        ]);
        s.model.set_bits("request_data", &Bits::from(t.words));
        s.until(1000, "public action ingress", |m| {
            m.get("request_ready") != 0
        });
        s.idle(1);
        s.model.set("request_valid", 0);
        s.until(1000, "public action reply", |m| {
            m.get("response_valid") != 0
        });
        let result = (
            s.model.get("response_error") as u8,
            s.model.bits("response_data").array(),
        );
        s.idle(1);
        result
    }
    fn service(s: &mut Sim<'_>, owner: &mut Service, mut t: Transfer) -> Progress {
        loop {
            let (error, row) = exchange(s, &t);
            match owner
                .finish(error, row, s.model.get("current_frame"))
                .unwrap()
            {
                Progress::Transfer(next) => t = next,
                result => return result,
            }
        }
    }
    fn start(s: &mut Sim<'_>, frame: u64, mode: u64) {
        s.model.set_all(&[
            ("current_frame", frame),
            ("cut_mode", mode),
            ("cut_start", 1),
        ]);
        s.idle(1);
        s.model.set("cut_start", 0);
    }
    fn next(s: &mut Sim<'_>, opcode: u64, mode: u64) {
        s.until(1000, "private resident delivery", |m| {
            m.get("resident_valid") != 0
        });
        assert_eq!(s.model.get("resident_opcode"), opcode);
        assert_eq!(s.model.get("resident_mode"), mode);
        s.model.set("resident_ready", 1);
        s.idle(1);
        s.model.set("resident_ready", 0);
    }
    fn finish(s: &mut Sim<'_>, pending: bool, data: &Bits) {
        s.model.set_bits("resident_result", data);
        s.model.set_all(&[
            ("resident_done", 1),
            ("resident_pending", u64::from(pending)),
        ]);
        s.idle(1);
        s.model
            .set_all(&[("resident_done", 0), ("resident_pending", 0)]);
    }
    fn lookup(s: &mut Sim<'_>, slot: u16) -> Bits {
        s.model
            .set_all(&[("lookup_valid", 1), ("lookup_slot", slot.into())]);
        s.until(10, "private lookup ready", |m| m.get("lookup_ready") != 0);
        s.idle(1);
        s.model.set("lookup_valid", 0);
        assert_eq!(s.model.get("lookup_done"), 1);
        assert_eq!(s.model.get("lookup_error"), 0);
        let row = s.model.bits("lookup_data");
        s.idle(1);
        row
    }
    fn update(s: &mut Sim<'_>, id: a::Identity, row: &Bits, error: u64) {
        s.model.set_all(&[
            ("update_valid", 1),
            ("update_slot", id.slot.into()),
            ("update_generation", id.generation.into()),
        ]);
        s.model.set_bits("update_data", row);
        s.until(10, "private write ready", |m| m.get("update_ready") != 0);
        s.idle(1);
        s.model.set("update_valid", 0);
        assert_eq!(
            s.model.get("update_done"),
            0,
            "writeback acknowledged before the synchronous RAM write"
        );
        s.until(10, "private write commit", |m| m.get("update_done") != 0);
        assert_eq!(s.model.get("update_error"), error);
        s.idle(1);
    }
    let mut owner = Service::new(4, 32)?;
    let cancel = Transfer {
        opcode: a::CANCEL,
        state: false,
        address: 0,
        words: {
            let mut w = [0; 20];
            w[0] = 42;
            w
        },
    };
    let transfer = owner.schedule(50, 3200, cancel).unwrap();
    let cancel_id = match service(&mut s, &mut owner, transfer) {
        Progress::Admitted { identity, .. } => identity,
        _ => panic!("cancel not admitted"),
    };
    start(&mut s, 0, 0);
    s.until(1000, "future cancel scan", |m| m.get("cut_done") != 0);
    assert_eq!(s.model.get("resident_reference") >> 16, 0);
    s.idle(1);
    let transition = Transfer {
        opcode: a::TRANSITION,
        state: false,
        address: 0,
        words: {
            let mut w = [0; 20];
            w[0] = 0x12345678;
            w[15] = 0x87654321;
            w
        },
    };
    let transfer = owner.schedule(42, 3200, transition.clone()).unwrap();
    let target = match service(&mut s, &mut owner, transfer) {
        Progress::Admitted { identity, .. } => identity,
        _ => panic!("target not admitted"),
    };
    start(&mut s, 0, 0);
    s.until(1000, "rebuilt cancellation references", |m| {
        m.get("cut_done") != 0
    });
    assert_eq!(
        s.model.get("resident_reference"),
        u64::from(target.slot) | (u64::from(target.generation) << 16)
    );
    s.idle(1);
    let (error, _) = exchange(&mut s, &a::cancel(target));
    assert_eq!(error, 3, "nonplanner cancellation targeted a planner");
    start(&mut s, 3200, 0);
    next(&mut s, a::TRANSITION.into(), 0);
    assert_eq!(
        s.model.bits("resident_data").array::<16>(),
        transition.words[..16]
    );
    let mut native = Bits::zero(512);
    native
        .set(32, 32, 1)
        .set(64, 1, 1)
        .set(65, 32, 1)
        .set(64 + 77, 64, 42);
    finish(&mut s, true, &native);
    next(&mut s, a::CANCEL.into(), 0);
    assert_eq!(
        s.model.get("resident_reference"),
        u64::from(target.slot) | (u64::from(target.generation) << 16)
    );
    let before = lookup(&mut s, target.slot);
    assert_eq!(before.get(169, 1), 0);
    assert_eq!(before.get(186, 1), 0);
    update(
        &mut s,
        a::Identity {
            generation: target.generation + 1,
            ..target
        },
        &before,
        3,
    );
    assert_eq!(
        s.model.get("update_ready"),
        0,
        "a used guard allowed a second write"
    );
    assert_eq!(lookup(&mut s, target.slot), before);
    native.set(64 + 37, 3, 3).set(64 + 333, 64, 3200);
    let mut completed = before.clone();
    completed.set(186, 1, 1).set(191, 64, 3200);
    for word in 0..16 {
        completed.set(255 + word * 32, 32, native.get(word * 32, 32));
    }
    update(&mut s, target, &completed, 0);
    assert_eq!(
        lookup(&mut s, target.slot),
        completed,
        "ACK preceded RAM commit"
    );
    finish(&mut s, false, &Bits::zero(512));
    s.until(1000, "delivery cut complete", |m| m.get("cut_done") != 0);
    s.idle(1);
    start(&mut s, 3200, 1);
    s.until(1000, "resolved terminal rows", |m| m.get("cut_done") != 0);
    s.idle(1);
    start(&mut s, 3232, 2);
    next(&mut s, a::TRANSITION.into(), 2);
    assert_eq!(s.model.bits("resident_data"), native);
    finish(&mut s, false, &Bits::zero(512));
    s.until(1000, "native-cell release cut", |m| m.get("cut_done") != 0);
    s.idle(1);
    let transfer = owner.inspect(target, false).unwrap();
    match service(&mut s, &mut owner, transfer) {
        Progress::Record {
            record, original, ..
        } => {
            assert!(record.terminal && record.state);
            assert_eq!(record.terminal_frame, 3200);
            assert_eq!(
                (
                    original.opcode,
                    original.state,
                    original.address,
                    original.words
                ),
                (
                    transition.opcode,
                    transition.state,
                    transition.address,
                    transition.words
                )
            );
        }
        _ => panic!("retained target missing"),
    }
    let transfer = owner.inspect(cancel_id, false).unwrap();
    match service(&mut s, &mut owner, transfer) {
        Progress::Record { record, .. } => assert!(record.terminal),
        _ => panic!("cancel result missing"),
    }
    let transfer = owner.inspect(target, true).unwrap();
    assert!(matches!(
        service(&mut s, &mut owner, transfer),
        Progress::Record { .. }
    ));
    assert_eq!(s.model.get("fault"), 0);
    println!(
        "public rows: private payload publication, later-admitted cancellation target rebinding, guarded mutation/ACK, pending/terminal/release modes and retained originals"
    );
    Ok(())
}

pub fn public_delivery_runs_without_arm_collection() -> Result<()> {
    use crate::drivers::transition as abi;
    use rt_backend_testbench::transition_fixture as production;
    use rt_fpga_adapter::{
        audio_actions::{self as a, Progress, Service},
        audio_image::Transfer,
        protocol,
    };
    fn exchange(s: &mut Sim<'_>, t: &Transfer) -> (u8, [u32; 26]) {
        s.model.set_all(&[
            ("control_valid", 1),
            ("control_opcode", t.opcode.into()),
            ("control_state", u64::from(t.state)),
            ("control_address", t.address.into()),
        ]);
        s.model.set_bits("control_data", &Bits::from(t.words));
        s.until(1000, "public control ready", |m| {
            m.get("control_ready") != 0
        });
        s.idle(1);
        s.model.set("control_valid", 0);
        s.until(2000, "public control reply", |m| m.get("control_done") != 0);
        let result = (
            s.model.get("control_error") as u8,
            s.model.bits("control_result").array(),
        );
        s.idle(1);
        result
    }
    fn service(s: &mut Sim<'_>, owner: &mut Service, mut t: Transfer) -> Progress {
        loop {
            let (error, row) = exchange(s, &t);
            match owner
                .finish(error, row, s.model.get("current_frame"))
                .unwrap()
            {
                Progress::Transfer(next) => t = next,
                result => return result,
            }
        }
    }
    fn cut(s: &mut Sim<'_>, mode: u64) {
        s.model.set_all(&[("cut_start", 1), ("cut_mode", mode)]);
        s.idle(1);
        s.model.set("cut_start", 0);
        s.until(14000, "bounded public cut", |m| {
            m.get("cut_done") != 0 || m.get("fault") != 0
        });
        assert_eq!(s.model.get("fault"), 0);
        s.idle(1);
    }
    fn observe(s: &mut Sim<'_>, kind: u64, index: u64) -> Bits {
        s.model.set_all(&[
            ("observe_valid", 1),
            ("observe_kind", kind),
            ("observe_index", index),
        ]);
        s.until(100, "test observer ready", |m| m.get("observe_ready") != 0);
        s.idle(1);
        s.model.set("observe_valid", 0);
        s.until(100, "test observation", |m| {
            m.get("observe_response_valid") != 0
        });
        assert_eq!(s.model.get("observe_error"), 0);
        let row = s.model.bits("observe_data");
        s.idle(1);
        row
    }
    fn admitted(
        s: &mut Sim<'_>,
        owner: &mut Service,
        sequence: u64,
        at: u64,
        t: Transfer,
    ) -> a::Identity {
        let transfer = owner.schedule(sequence, at, t).unwrap();
        match service(s, owner, transfer) {
            Progress::Admitted { identity, .. } => identity,
            _ => panic!("public admission did not complete"),
        }
    }
    fn retained(s: &mut Sim<'_>, owner: &mut Service, id: a::Identity) -> a::Record {
        let t = owner.inspect(id, false).unwrap();
        match service(s, owner, t) {
            Progress::Record { record, .. } => record,
            _ => panic!("public retention did not complete"),
        }
    }
    fn native(command: &serde_json::Value) -> Transfer {
        Transfer {
            opcode: a::TRANSITION,
            state: false,
            address: 0,
            words: wire::install(&abi::command(command).array(), 0),
        }
    }
    let root = crate::root();
    let compiled = Design::new(
        "public_native_probe",
        vec![
            root.join("fpga/loop_pipeline/tests/public_native_probe.sv"),
            root.join("fpga/loop_pipeline/rtl/native_requests.sv"),
            root.join("fpga/audio_io/rtl/audio_actions.sv"),
            root.join("fpga/audio_io/rtl/audio_native.sv"),
            root.join("fpga/state_processor/rtl/sp_ram.sv"),
            root.join("fpga/transition_processor/rtl/transition_processor.sv"),
            root.join("fpga/transition_processor/rtl/tp_ram.sv"),
            root.join("fpga/transition_processor/rtl/tp_priority.sv"),
        ],
    )
    .probes(vec![Probe::observe("timing.event_count")])
    .build(&root.join("work/marlin-suite/public-native"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10000, 0)]);
    let mut d = Transition::new([1, 4, 6]);
    let mut commands = vec![];
    let steps = (0..1040u64)
        .map(|q| {
            let mut due = vec![];
            if q % 32 == 0 && commands.len() < 32 {
                let seq = commands.len() as u64 + 1;
                let command = super::transition::command(
                    0,
                    seq,
                    if seq == 1 {
                        super::transition::play(0, true)
                    } else {
                        super::transition::seek((seq % 4) * 16, true)
                    },
                    q * 16,
                    None,
                    0,
                );
                commands.push(command.clone());
                due.push(command);
            }
            super::transition::step(due)
        })
        .collect();
    let case = super::transition::case(
        "public-resident",
        vec![super::transition::spec(0, 64, Some(0), true)],
        steps,
        16,
        Some(1),
        4,
    );
    let trace = production::run_transition_oracle(serde_json::from_value(case.clone())?)
        .map_err(|e| format!("{e:?}"))?;
    let image = production::compile(&serde_json::from_value(case["plan"].clone())?)
        .map_err(|e| format!("{e:?}"))?;
    d.load(&mut s, &serde_json::to_value(image)?);
    d.service = None;
    let mut owner = Service::new(64, 16)?;
    let ids: Vec<_> = commands
        .iter()
        .map(|command| {
            admitted(
                &mut s,
                &mut owner,
                command["sequence"].as_u64().unwrap(),
                command["at"].as_u64().unwrap(),
                native(command),
            )
        })
        .collect();
    let held = d.hold_raw_command(&mut s, 0, 0, &Bits::default(), false);
    for (q, expected) in trace.results.iter().enumerate() {
        cut(&mut s, 0);
        execute(&mut s);
        cut(&mut s, 1);
        assert_eq!(s.model.get("control_valid"), 0);
        assert_eq!(s.model.get("response_valid"), 1);
        assert_eq!(s.model.get("response_error"), held.0);
        assert_eq!(s.model.bits("response_data"), held.1);
        assert_eq!(s.model.get("current_frame"), expected.next_frame);
        // The separate read-only test observer does not service the public owner.
        assert_eq!(
            abi::state(&observe(&mut s, 0, 0), 0),
            serde_json::to_value(&expected.states[0])?,
            "state at quantum {q}"
        );
        assert_eq!(
            s.model.get("timing.event_count"),
            expected.events.len() as u64,
            "event count at quantum {q}"
        );
        for (i, event) in expected.events.iter().enumerate() {
            assert_eq!(
                abi::event(&observe(&mut s, 2, i as u64)),
                serde_json::to_value(event)?,
                "event {i} at quantum {q}"
            );
        }
        cut(&mut s, 2);
    }
    for (index, (command, id)) in commands.iter().zip(ids).enumerate() {
        let record = retained(&mut s, &mut owner, id);
        assert!(record.terminal && record.state);
        assert_eq!(record.error, 0);
        assert_eq!(record.terminal_frame, command["at"].as_u64().unwrap());
        assert_eq!(
            protocol::get(&record.payload, 0, 32),
            0,
            "retained row {record:?}"
        );
        assert_eq!(protocol::get(&record.payload, 32, 32), index as u64 + 1);
        let compact: [u32; 26] =
            std::array::from_fn(|i| Bits::from(record.payload).get(64 + i * 32, 32) as u32);
        let original = abi::command(command).array();
        let decoded = wire::result(&compact, Some(&original), index as u32 + 1).unwrap();
        let expected = &trace.results[index * 32].requests[0];
        assert_eq!(
            abi::request(&Bits::from(decoded), 0),
            serde_json::to_value(expected)?
        );
    }
    // Two cancellations in one cut both see the still-pending native request.
    let frame = s.model.get("current_frame");
    let command = super::transition::command(
        0,
        100,
        super::transition::play(0, true),
        frame,
        Some(0),
        100,
    );
    let target = admitted(&mut s, &mut owner, 100, frame, native(&command));
    let cancel = |target| Transfer {
        opcode: a::CANCEL,
        state: false,
        address: 0,
        words: {
            let mut w = [0; 20];
            protocol::put(&mut w, 0, 64, target);
            w
        },
    };
    let c1 = admitted(&mut s, &mut owner, 101, frame, cancel(100));
    let c2 = admitted(&mut s, &mut owner, 102, frame, cancel(100));
    cut(&mut s, 0);
    execute(&mut s);
    cut(&mut s, 1);
    cut(&mut s, 2);
    let row = retained(&mut s, &mut owner, target);
    assert_eq!(protocol::get(&row.payload, 64 + 37, 3), 3);
    for id in [c1, c2] {
        let row = retained(&mut s, &mut owner, id);
        assert_eq!(protocol::get(&row.payload, 504, 8), 9);
        assert_eq!(protocol::get(&row.payload, 64, 1), 1);
    }
    let frame = s.model.get("current_frame");
    let terminal_cancel = admitted(&mut s, &mut owner, 103, frame, cancel(100));
    let self_cancel = admitted(&mut s, &mut owner, 104, frame, cancel(104));
    let mut recovery = cancel(105);
    recovery.address = 4;
    let recovery_self = admitted(&mut s, &mut owner, 105, frame, recovery);
    let unknown = admitted(&mut s, &mut owner, 106, frame, cancel(999));
    cut(&mut s, 0);
    execute(&mut s);
    cut(&mut s, 1);
    cut(&mut s, 2);
    for (id, error) in [
        (terminal_cancel, 6),
        (self_cancel, 6),
        (recovery_self, 1),
        (unknown, 1),
    ] {
        let row = retained(&mut s, &mut owner, id);
        assert_eq!(protocol::get(&row.payload, 504, 8), 1);
        assert_eq!(protocol::get(&row.payload, 0, 8), error);
    }
    let frame = s.model.get("current_frame");
    for sequence in 200..204 {
        let command = super::transition::command(
            0,
            sequence,
            super::transition::play(0, true),
            frame,
            Some(0),
            1000,
        );
        admitted(&mut s, &mut owner, sequence, frame, native(&command));
    }
    let command =
        super::transition::command(0, 210, super::transition::play(0, true), frame, None, 0);
    let mut bad_target = native(&command);
    protocol::put(&mut bad_target.words, wire::TARGET, 64, 1 | (3 << 32));
    protocol::put(&mut bad_target.words, 496, 8, 2);
    let bad = admitted(&mut s, &mut owner, 210, frame, bad_target);
    let command =
        super::transition::command(0, 211, super::transition::play(0, true), frame, None, 0);
    let mut deferred = native(&command);
    protocol::put(&mut deferred.words, 496, 8, 2);
    let deferred = admitted(&mut s, &mut owner, 211, frame, deferred);
    let command =
        super::transition::command(0, 212, super::transition::play(0, true), frame, None, 0);
    let full = admitted(&mut s, &mut owner, 212, frame, native(&command));
    cut(&mut s, 0);
    execute(&mut s);
    cut(&mut s, 1);
    cut(&mut s, 2);
    for (id, tag, reason) in [(bad, 2, 3), (deferred, 1, 2), (full, 2, 2)] {
        let row = retained(&mut s, &mut owner, id);
        assert_eq!(protocol::get(&row.payload, 504, 8), tag);
        assert_eq!(
            protocol::get(&row.payload, 0, 8),
            reason,
            "dynamic target/deferred validation/native capacity precedence"
        );
    }
    let frame = s.model.get("current_frame");
    for sequence in 200..204 {
        admitted(&mut s, &mut owner, sequence + 14, frame, cancel(sequence));
    }
    cut(&mut s, 0);
    execute(&mut s);
    cut(&mut s, 1);
    cut(&mut s, 2);
    let frame = s.model.get("current_frame");
    let command =
        super::transition::command(0, 220, super::transition::play(0, true), frame, None, 0);
    let after_full = admitted(&mut s, &mut owner, 220, frame, native(&command));
    cut(&mut s, 0);
    execute(&mut s);
    cut(&mut s, 1);
    cut(&mut s, 2);
    let row = retained(&mut s, &mut owner, after_full);
    assert_eq!(
        protocol::get(&row.payload, 504, 8),
        0,
        "native full cache survived successful collection"
    );
    assert_eq!(protocol::get(&row.payload, 0, 32), 0);
    assert_eq!(s.model.get("fault"), 0);
    println!(
        "1040 quanta with ARM owner paused and host reply held: 32 retained public results reuse 4 native cells; all states/events match production; duplicate, terminal, self, recovery and unknown cancellations preserve reasons"
    );
    Ok(())
}
