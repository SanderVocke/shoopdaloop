fn u32_at(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn u64_at(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn event(frame: u64, target: u32, code: u8, flag: u8, a: u64, b: u64) -> [u8; 160] {
    let mut bytes = [0; 160];
    bytes[0] = 3;
    bytes[1] = code;
    bytes[2] = flag;
    u64_at(&mut bytes, 8, frame);
    u32_at(&mut bytes, 16, target);
    u32_at(&mut bytes, 20, 1);
    u64_at(&mut bytes, 24, a);
    u64_at(&mut bytes, 32, b);
    bytes
}
// Closed-form fixture table, not a call to the resolver or its journal encoder.
pub(super) fn semantic(stalled: bool) -> Vec<u8> {
    let mut bytes = vec![0; 40];
    bytes[..8].copy_from_slice(b"LSJ1\x01\0\xa0\0");
    for frame in 0u64..if stalled { 136 } else { 131 } {
        let mut events = Vec::new();
        if frame == 0 {
            for (target, code, flag) in [
                (0, 1, 0),
                (0, 10, 0),
                (1, 2, 0),
                (1, 10, 0),
                (3, 2, 0),
                (3, 10, 0),
                (3, 12, 0),
                (4, 6, 1),
                (4, 10, 0),
                (4, 12, 0),
                (2, 2, 0),
                (2, 10, 0),
            ] {
                events.push(event(frame, target, code, flag, 0, 0));
            }
        } else {
            match frame {
                1 | 131 => {
                    events.push(event(frame, 0, 2, 0, 0, 0));
                    events.push(event(frame, 0, 10, 0, 0, 0));
                }
                64 => events.push(event(frame, 0, 4, 0, 0, 0)),
                129 | 135 => events.push(event(frame, 0, 3, 0, 0, 0)),
                130 => {}
                _ => {
                    events.push(event(frame, 0, 9, 0, 0, 0));
                    events.push(event(frame, 0, 10, 0, 0, 0));
                }
            }
            events.push(event(frame, 1, 9, 0, 0, 0));
            events.push(event(frame, 1, 10, 0, 0, 0));
            events.push(event(frame, 3, 10, 0, 0, 0));
            events.push(event(frame, 3, 12, 0, frame % 4, frame / 4));
            events.push(event(frame, 4, 10, 0, 0, 0));
            if frame < 4 {
                events.push(event(frame, 4, 12, 0, frame, 0));
            } else if frame == 4 {
                events.push(event(frame, 4, 13, 0, 0, 0));
            }
            events.push(event(frame, 2, 9, 0, 0, 0));
            events.push(event(frame, 2, 10, 0, 0, 0));
        }
        let mut boundary = [0; 160];
        boundary[0] = 1;
        u64_at(&mut boundary, 8, frame);
        u32_at(&mut boundary, 16, 5);
        u32_at(&mut boundary, 20, events.len() as u32);
        bytes.extend_from_slice(&boundary);
        for target in 0..5 {
            let mut decision = [0; 160];
            decision[0] = 2;
            u64_at(&mut decision, 8, frame);
            u32_at(&mut decision, 16, target);
            u32_at(&mut decision, 20, 1);
            let (mode, flags, fields) = match target {
                0 => {
                    let cycles = if frame >= 131 {
                        frame.min(134) - 131
                    } else if frame < 2 {
                        0
                    } else if frame < 64 {
                        frame - 1
                    } else {
                        frame.min(128) - 2
                    };
                    let pulse = match frame {
                        64 => 63,
                        129..=130 => 128,
                        135 => 134,
                        _ => frame,
                    };
                    (
                        if frame == 0 {
                            2
                        } else if frame < 129 || (131..135).contains(&frame) {
                            1
                        } else {
                            0
                        },
                        match frame {
                            0 | 129 | 135 => 72,
                            1 | 64 | 131 => 73,
                            130 => 8,
                            _ => 9,
                        },
                        [
                            u64::from(frame > 0),
                            u64::from(frame > 0),
                            0,
                            cycles,
                            u64::from(frame > 0),
                            1,
                            pulse,
                            0,
                            0,
                            0,
                        ],
                    )
                }
                1 | 2 => (1, 9, [1, 0, 0, frame, 0, 0, frame, 0, 0, 0]),
                3 => (
                    1,
                    56,
                    [4, 0, 0, 0, 0, 0, frame, frame % 4, frame / 4, frame],
                ),
                4 => (
                    if frame < 4 { 1 } else { 0 },
                    56,
                    [
                        4,
                        0,
                        0,
                        0,
                        0,
                        0,
                        frame,
                        if frame < 4 { frame } else { 0 },
                        u64::from(frame >= 4),
                        frame.min(4),
                    ],
                ),
                _ => unreachable!(),
            };
            decision[1] = mode;
            decision[2] = flags;
            for (index, value) in fields.into_iter().enumerate() {
                u64_at(&mut decision, 24 + index * 8, value);
            }
            bytes.extend_from_slice(&decision);
        }
        for event in events {
            bytes.extend_from_slice(&event);
        }
    }
    let count = (bytes.len() - 40) / 160;
    u32_at(&mut bytes, 8, count as u32);
    bytes
}
