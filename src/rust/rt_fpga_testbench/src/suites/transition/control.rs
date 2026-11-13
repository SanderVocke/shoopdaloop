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
