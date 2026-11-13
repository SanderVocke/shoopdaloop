use crate::export_wire::{put32, put64};
fn event(frame: u64, target: u32, code: u8, a: u64, b: u64) -> [u8; 160] {
    let mut r = [0; 160];
    r[0] = 3;
    r[1] = code;
    put64(&mut r, 8, frame);
    put32(&mut r, 16, target);
    put32(&mut r, 20, 1);
    put64(&mut r, 24, a);
    put64(&mut r, 32, b);
    r
}
pub(super) fn semantic(frames: u64) -> Vec<u8> {
    let mut bytes = vec![0; 40];
    bytes[..8].copy_from_slice(b"LSJ1\x01\0\xa0\0");
    for frame in 0..frames {
        let mut events = Vec::new();
        if frame == 49 {
            events.push(event(frame, 0, 11, 0, 0));
        }
        events.push(event(frame, 1, if frame == 0 { 2 } else { 9 }, 0, 0));
        events.push(event(frame, 1, 10, 0, 0));
        match frame {
            0 => events.push(event(frame, 0, 2, 0, 0)),
            4 => events.push(event(frame, 0, 5, 3, 7)),
            6 => events.push(event(frame, 0, 4, 1, 0)),
            8 => events.push(event(frame, 0, 9, 0, 0)),
            9 => events.push(event(frame, 0, 1, 0, 0)),
            _ => {}
        }
        events.push(event(frame, 0, 10, 0, 0));
        let mut boundary = [0; 160];
        boundary[0] = 1;
        put64(&mut boundary, 8, frame);
        put32(&mut boundary, 16, 2);
        put32(&mut boundary, 20, events.len() as u32);
        bytes.extend(boundary);
        for target in 0..2 {
            let mut r = [0; 160];
            r[0] = 2;
            put64(&mut r, 8, frame);
            put32(&mut r, 16, target);
            put32(&mut r, 20, 1);
            let (mode, flags, fields) = if target == 1 {
                (
                    1,
                    if frame == 0 { 73 } else { 9 },
                    [1, 0, 0, frame, 0, 0, frame],
                )
            } else {
                let position = match frame {
                    0..=3 => frame,
                    4 | 8 => 0,
                    5 | 6 => 1,
                    7 => 2,
                    9..=48 => 1,
                    _ => 0,
                };
                (
                    if frame < 9 {
                        1
                    } else if frame < 49 {
                        2
                    } else {
                        0
                    },
                    match frame {
                        0 | 4 | 6 => 73,
                        8 => 9,
                        9 => 72,
                        49.. => 10,
                        _ => 8,
                    },
                    [
                        if frame < 4 { 4 } else { 3 },
                        if frame < 4 { 0 } else { 7 },
                        position,
                        u64::from(frame >= 8),
                        frame.saturating_sub(9).min(40),
                        if frame < 9 { 0 } else { 8 },
                        frame,
                    ],
                )
            };
            r[1] = mode;
            r[2] = flags;
            for (i, value) in fields.into_iter().enumerate() {
                put64(&mut r, 24 + i * 8, value);
            }
            bytes.extend(r);
        }
        for r in events {
            bytes.extend(r);
        }
    }
    let count = ((bytes.len() - 40) / 160) as u32;
    put32(&mut bytes, 8, count);
    bytes
}
pub(super) fn midi() -> Vec<u8> {
    fn packet(out: &mut Vec<u8>, frame: u64, packet: [u8; 3]) {
        out.extend(frame.to_le_bytes());
        out.extend(3u32.to_le_bytes());
        out.extend(packet);
    }
    fn restore(out: &mut Vec<u8>, frame: u64, held: &[(u8, u8)]) {
        for channel in 0..16 {
            packet(out, frame, [0xe0 | channel, 0, 64]);
            for cc in [64, 69, 66] {
                packet(out, frame, [0xb0 | channel, cc, 0]);
            }
            if channel == 0 {
                for &(key, velocity) in held {
                    packet(out, frame, [0x90, key, velocity]);
                }
            }
        }
    }
    let mut out = vec![];
    packet(&mut out, 0, [0x90, 60, 100]);
    restore(&mut out, 0, &[(55, 31)]);
    packet(&mut out, 0, [0x90, 61, 100]);
    packet(&mut out, 2, [0x80, 60, 45]);
    packet(&mut out, 2, [0x80, 61, 45]);
    packet(&mut out, 4, [0x80, 55, 0]);
    restore(&mut out, 4, &[(70, 50)]);
    packet(&mut out, 4, [0x90, 62, 90]);
    packet(&mut out, 6, [0x80, 62, 0]);
    packet(&mut out, 6, [0x80, 70, 0]);
    restore(&mut out, 6, &[(62, 90), (70, 50)]);
    packet(&mut out, 7, [0x80, 62, 45]);
    packet(&mut out, 8, [0x80, 70, 0]);
    restore(&mut out, 8, &[(70, 50)]);
    packet(&mut out, 8, [0x90, 62, 90]);
    packet(&mut out, 9, [0x80, 62, 0]);
    packet(&mut out, 9, [0x80, 70, 0]);
    out
}
