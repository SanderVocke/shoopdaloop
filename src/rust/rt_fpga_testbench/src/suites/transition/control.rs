use super::*;
use rt_fpga_adapter::{looping::TpReply, protocol, transition_service::TransitionService};

fn execute(s: &mut Sim<'_>, response: &(u64, Bits), expected_error: u64) {
    s.model.set_all(&[
        ("execute_valid", 1),
        ("execute_finish", 0),
        ("command_valid", 1),
        ("command_opcode", 255),
    ]);
    s.model.eval();
    assert_eq!(s.model.get("execute_ready"), 1);
    s.idle(1);
    s.model.set("execute_valid", 0);
    let mut done = false;
    for _ in 0..100_000 {
        assert_eq!(s.model.get("response_valid"), 1);
        assert_eq!(s.model.get("response_error"), response.0);
        assert_eq!(s.model.bits("response_data"), response.1);
        assert_eq!(s.model.get("command_ready"), 0);
        if s.model.get("execute_done") != 0 {
            done = true;
            break;
        }
        s.idle(1);
    }
    assert!(done, "execution waited for a held host response");
    assert_eq!(s.model.get("execute_error"), expected_error);
    s.model.set("command_valid", 0);
}

pub(super) fn held_replies(d: &mut Transition, s: &mut Sim<'_>) -> Result<()> {
    let image = case(
        "held-replies",
        vec![spec(0, 64, None, false)],
        vec![],
        16,
        Some(1),
        1,
    );
    for opcode in [4, 5, 6, 13] {
        load(d, s, &image)?;
        let public = abi::command(&command(
            0,
            1,
            json!({"kind":"record"}),
            if opcode == 5 { 1024 } else { 0 },
            None,
            0,
        ));
        let original: [u32; 20] = public.0.clone().try_into().unwrap();
        if opcode != 4 {
            d.ok(s, 4, 0, &public);
            let mut extra = original;
            protocol::put(&mut extra, 128, 64, 99);
            assert_eq!(d.service.as_mut().unwrap().begin(4, 0, extra), Err(2));
            if opcode == 6 {
                d.ok(s, 9, 0, &Bits::default());
            }
        }
        let mut owner: TransitionService = d.service.take().unwrap();
        let mut data = [0; 20];
        data[0] = 1;
        let transfer = owner
            .begin(opcode, 0, if opcode == 4 { original } else { data })
            .unwrap();
        let transfer = if opcode == 4 {
            abi::ready_install(&mut owner, d, s, transfer)
        } else {
            transfer
        };
        let held = d.hold_raw_command(
            s,
            u64::from(transfer.opcode),
            u64::from(transfer.index),
            &Bits(transfer.words.to_vec()),
            false,
        );
        assert_eq!(held.0, 0);
        for _ in 0..8 {
            execute(s, &held, 0);
        }
        assert_eq!(s.model.get("frame"), if opcode == 6 { 144 } else { 128 });
        s.model.set("response_ready", 1);
        s.idle(1);
        s.model.set("response_ready", 0);
        owner
            .finish(TpReply {
                error: held.0 as u8,
                words: held.1.0.try_into().unwrap(),
            })
            .unwrap();
        d.service = Some(owner);
        let row = d.ok(s, 13, 0, &Bits::default());
        if opcode == 6 {
            assert_eq!((row.get(0, 1), row.get(1, 32)), (0, 2));
            let next = abi::command(&command(0, 99, json!({"kind":"stop"}), 0, None, 0));
            let id = d.ok(s, 4, 0, &next);
            assert_eq!((id.get(0, 32), id.get(32, 32)), (0, 2));
        } else {
            assert_eq!(row.get(37, 3), if opcode == 5 { 3 } else { 1 });
            d.ok(s, 6, 0, &Bits::from_u64(1));
        }
    }
    let sync = case(
        "held-reuse",
        vec![spec(0, 16, Some(0), true)],
        vec![],
        16,
        Some(1),
        1,
    );
    load(d, s, &sync)?;
    let play = abi::command(&command(0, 1, play(0, true), 0, None, 0));
    d.ok(s, 4, 0, &play);
    d.ok(s, 9, 0, &Bits::default());
    d.ok(s, 6, 0, &Bits::from_u64(1));
    d.ok(
        s,
        4,
        0,
        &abi::command(&command(0, 2, super::play(0, true), 0, Some(0), 0)),
    );
    d.ok(s, 9, 0, &Bits::default());
    let mut owner = d.service.take().unwrap();
    let transfer = owner
        .begin(6, 0, {
            let mut w = [0; 20];
            w[0] = 2;
            w
        })
        .unwrap();
    let held = d.hold_raw_command(s, 6, 0, &Bits(transfer.words.to_vec()), false);
    assert_eq!(held.1.get(37, 3), 1);
    for _ in 0..8 {
        execute(s, &held, 0);
    }
    s.model.set("response_ready", 1);
    s.idle(1);
    s.model.set("response_ready", 0);
    owner
        .finish(TpReply {
            error: 0,
            words: held.1.0.try_into().unwrap(),
        })
        .unwrap();
    d.service = Some(owner);
    let empty = d.ok(s, 13, 0, &Bits::default());
    assert_eq!((empty.get(0, 1), empty.get(1, 32)), (0, 3));
    let id = d.ok(
        s,
        4,
        0,
        &abi::command(&command(0, 3, json!({"kind":"record"}), 0, Some(0), 2)),
    );
    assert_eq!(id.get(32, 32), 3);
    for remaining in [1, 0] {
        d.ok(s, 9, 0, &Bits::default());
        let pending = d.ok(s, 13, 0, &Bits::default());
        assert_eq!((pending.get(37, 3), pending.get(512, 64)), (0, remaining));
    }
    d.ok(s, 9, 0, &Bits::default());
    assert_eq!(d.ok(s, 13, 0, &Bits::default()).get(37, 3), 1);
    load(d, s, &image)?;
    let held = d.hold_raw_command(s, 0, 0, &Bits::default(), false);
    s.model.inject("frame", Some(&Bits::from_u64(u64::MAX)));
    execute(s, &held, 8);
    s.model.inject("frame", None);
    d.reset(s);
    assert_eq!(s.model.get("response_valid"), 0);
    assert_eq!(s.model.get("execute_done"), 0);
    assert_eq!(s.model.get("execute_error"), 0);
    Ok(())
}

pub(super) fn publication(d: &mut Transition, s: &mut Sim<'_>) -> Result<()> {
    // This raw campaign deliberately bypasses the trusted membership compiler.
    let image = case(
        "publication-guards",
        vec![spec(0, 64, None, false)],
        vec![],
        16,
        Some(1),
        1,
    );
    load(d, s, &image)?;
    d.ok(
        s,
        4,
        0,
        &abi::command(&command(0, 1, json!({"kind":"record"}), 0, None, 0)),
    );
    let raw = |d: &mut Transition, s: &mut Sim<'_>, op, index, words: Vec<u32>, error| {
        let reply = d.raw_command(s, op, index, &Bits(words), Some(3), true);
        assert_eq!(reply.0, error, "raw publication opcode {op}");
        reply.1
    };
    raw(d, s, 18, 0, vec![0, 0, 1], 3);
    raw(d, s, 18, 0, vec![1, 0, 2], 2);
    let held = d.hold_raw_command(s, 18, 0, &Bits(vec![1, 0, 1]), false);
    assert_eq!((held.0, held.1.get(0, 64)), (0, 2));
    for _ in 0..8 {
        execute(s, &held, 0);
    }
    s.model.set("response_ready", 1);
    s.idle(1);
    s.model.set("response_ready", 0);
    let terminal = raw(d, s, 13, 0, vec![], 0);
    assert_eq!(terminal.get(37, 3), 1);
    raw(d, s, 19, 0, vec![1, 0, 1, u32::MAX], 3);
    raw(d, s, 21, 0, vec![2], 5);
    raw(d, s, 20, 0, vec![2, 0, 0, 0, u32::MAX, 0], 5);
    raw(d, s, 19, 0, vec![2, 0, 2, u32::MAX], 3);
    raw(d, s, 19, 1, vec![2, 0, 1, u32::MAX], 3);
    // Strictly increasing successors rule out cycles.
    raw(d, s, 19, 0, vec![2, 0, 1, 0], 1);
    raw(d, s, 19, 0, vec![2, 0, 1, u32::MAX], 0);
    raw(d, s, 19, 0, vec![2, 0, 1, 0], 2);
    raw(d, s, 20, 1, vec![2, 0, 0, 0, 0, 0], 3);
    raw(d, s, 20, 0, vec![2, 0, 1, 0, u32::MAX, 0], 3);
    raw(d, s, 20, 0, vec![2, 0, 0, 0, u32::MAX, 0], 0);
    raw(d, s, 21, 0, vec![1], 3);
    raw(d, s, 21, 0, vec![2], 0);
    assert_eq!(raw(d, s, 13, 0, vec![], 0), terminal);
    raw(d, s, 21, 0, vec![2], 5);
    raw(d, s, 6, 0, vec![1], 0);
    let mut install = rt_fpga_adapter::transition_service::wire::install(
        &abi::command(&command(0, 2, json!({"kind":"record"}), 0, None, 0))
            .0
            .try_into()
            .unwrap(),
        2,
    );
    protocol::put(
        &mut install,
        rt_fpga_adapter::transition_service::wire::PUBLICATION,
        64,
        2,
    );
    raw(d, s, 4, 0, install.to_vec(), 3);
    s.model
        .inject("stage_counter", Some(&Bits::from_u64(u64::MAX)));
    raw(d, s, 18, 0, vec![2, 0, 0], 8);
    s.model.inject("stage_counter", None);
    for collect_before_commit in [false, true] {
        load(d, s, &image)?;
        d.ok(
            s,
            4,
            0,
            &abi::command(&command(0, 1, json!({"kind":"record"}), 1024, None, 0)),
        );
        raw(d, s, 18, 0, vec![1, 0, 1], 0);
        raw(d, s, 19, 0, vec![2, 0, 1, u32::MAX], 0);
        assert_eq!(raw(d, s, 5, 0, vec![1], 0).get(0, 1), 1);
        raw(d, s, 20, 0, vec![2, 0, 0, 0, u32::MAX, 0], 0);
        if collect_before_commit {
            raw(d, s, 6, 0, vec![1], 0);
        }
        raw(d, s, 21, 0, vec![2], 0);
        let row = raw(d, s, 13, 0, vec![], 0);
        if collect_before_commit {
            assert_eq!((row.get(0, 1), row.get(1, 32)), (0, 2));
        } else {
            assert_eq!(row.get(37, 3), 3);
            raw(d, s, 6, 0, vec![1], 0);
        }
        raw(d, s, 4, 0, install.to_vec(), 3);
    }
    for cut in 0..4 {
        load(d, s, &image)?;
        raw(d, s, 18, 0, vec![0, 0, 1], 0);
        if cut >= 1 {
            raw(d, s, 19, 0, vec![1, 0, 1, u32::MAX], 0);
        }
        if cut >= 2 {
            raw(d, s, 20, 0, vec![1, 0, 0, 0, u32::MAX, 0], 0);
        }
        if cut >= 3 {
            raw(d, s, 21, 0, vec![1], 0);
        }
        load(d, s, &image)?;
        raw(d, s, 19, 0, vec![1, 0, 1, 0], 5);
        let id = d.ok(
            s,
            4,
            0,
            &abi::command(&command(0, 3, json!({"kind":"record"}), 0, None, 0)),
        );
        assert_eq!(id.get(32, 32), 1);
    }
    Ok(())
}
