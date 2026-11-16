//! Packed port observations; behavioral expectations come from production code or explicit vectors.
use crate::{Result, bits::Bits};
use serde_json::{Value, json};

pub fn instruction(value: &Value) -> Result<Bits> {
    if value["kind"] == "process_loop" {
        let mut bits = Bits::zero(96);
        let slot = value["loop_id"]["slot"]
            .as_u64()
            .ok_or("missing loop slot")?;
        let generation = value["loop_id"]["generation"]
            .as_u64()
            .ok_or("missing loop generation")?;
        if slot > 65535 || generation > u32::MAX as u64 {
            return Err("loop identity does not fit instruction".into());
        }
        bits.set(0, 8, 12).set(8, 16, slot).set(24, 32, generation);
        return Ok(bits);
    }
    if let Ok(value) = serde_json::from_value::<crate::component::Instruction>(value.clone()) {
        return crate::component::instruction_words(value).map(Bits::from);
    }
    let (opcode, fields): (u64, &[&str]) = match value["kind"].as_str() {
        Some("process_loop_playback") => {
            (1, &["view", "position", "length", "enabled", "destination"])
        }
        Some("process_loop_capture") => (4, &["view", "enabled", "source"]),
        _ => return Err(format!("invalid instruction: {value}").into()),
    };
    let mut bits = Bits::zero(96);
    bits.set(0, 8, opcode);
    for (i, &field) in fields.iter().enumerate() {
        let n = value[field]
            .as_u64()
            .filter(|&n| n <= 65535)
            .ok_or_else(|| format!("unrepresentable operand {field}: {}", value[field]))?;
        bits.set(8 + i * 16, 16, n);
    }
    Ok(bits)
}
pub fn state(value: &Value) -> Bits {
    let state: crate::component::Value = serde_json::from_value(value.clone()).unwrap();
    crate::component::value_words(state).into()
}
pub fn decode_state(bits: &Bits) -> Value {
    if bits.get(128, 1) != 0 {
        json!({"kind":"ramp","value":ramp(bits,0)})
    } else {
        json!({"kind":"word","value":bits.get(0,64)})
    }
}
fn ramp(bits: &Bits, offset: usize) -> Value {
    json!({"start":bits.get(offset,32) as i32,"target":bits.get(offset+32,32) as i32,"duration":bits.get(offset+64,32),"elapsed":bits.get(offset+96,32)})
}
pub fn job(bits: &Bits) -> Value {
    let kind = bits.get(0, 8);
    let a = bits.get(8, 16);
    let b = bits.get(24, 16);
    let p0 = bits.get(64, 64);
    let p1 = bits.get(128, 64);
    let p2 = bits.get(192, 64);
    assert_eq!(bits.get(40, 24), 0, "nonzero reserved job bits");
    match kind {
        0 => {
            assert_eq!((p0, p1, p2), (0, 0, 0));
            json!({"kind":"input","port":a,"destination":b})
        }
        1 => {
            assert_eq!((b, p0, p1, p2), (0, 0, 0, 0));
            json!({"kind":"silence","destination":a})
        }
        2 => {
            assert_eq!(b, 0);
            json!({"kind":"read_view","destination":a,"view":p0,"position_chunks":p1,"length_chunks":p2})
        }
        3 => {
            assert_eq!(b <= 65535, true);
            assert_eq!(p2, 0);
            json!({"kind":"gain","source":a,"destination":b,"ramp":ramp(bits,64)})
        }
        4 => {
            assert!(p0 <= 65535);
            assert_eq!((p1, p2), (0, 0));
            json!({"kind":"mix","left":a,"right":b,"destination":p0})
        }
        5 => {
            assert_eq!((b, p1, p2), (0, 0, 0));
            json!({"kind":"capture","source":a,"view":p0})
        }
        6 => {
            assert_eq!((p0, p1, p2), (0, 0, 0));
            json!({"kind":"output","source":a,"port":b})
        }
        7 => {
            assert_eq!((a, b, p1, p2), (0, 0, 0, 0));
            json!({"kind":"begin_capture","view":p0})
        }
        8 => {
            assert_eq!((b, p1, p2), (0, 0, 0));
            assert_eq!(p0 >> 32, 0);
            json!({"kind":"constant","value":p0 as i32,"destination":a})
        }
        9 => {
            assert_eq!((p0 >> 32, p1, p2), (0, 0, 0));
            let mode =
                ["multiply", "left_balance", "right_balance", "one_minus"][(p0 >> 16) as usize];
            json!({"kind":"arithmetic","left":a,"right":b,"destination":p0 & 65535,"mode":mode})
        }
        10 => {
            assert_eq!((p1 >> 2, p2), (0, 0));
            json!({"kind":"smoothing","source":a,"destination":b,"history":p0 as u32,"duration":p0 >> 32,"playing":p1 & 1 != 0,"restart":p1 & 2 != 0})
        }
        _ => panic!("unknown job opcode {kind}"),
    }
}
