use crate::export_wire::{put32, put64};
pub(super) fn semantic(start: u64) -> Vec<u8> {
    let mut records = Vec::<[u8; 160]>::new();
    for (cut, events) in [
        (0, vec![(1, 0, false, 0), (10, 0, false, 0)]),
        (4, vec![(2, 0, false, 0), (10, 0, false, 0)]),
        (8, vec![(9, 0, false, 0), (10, 0, false, 0)]),
        (12, vec![(3, 0, false, 0)]),
        (
            start,
            vec![
                (2, 1, false, 0),
                (10, 1, false, 0),
                (12, 1, false, 0),
                (6, 2, true, 0),
                (10, 2, false, 0),
                (12, 2, false, 0),
                (2, 0, true, 0),
                (10, 0, false, 0),
            ],
        ),
        (
            start + 4,
            vec![
                (7, 1, false, 1),
                (12, 1, false, 1),
                (7, 2, false, 0),
                (12, 2, false, 0),
                (4, 0, false, 0),
            ],
        ),
        (
            start + 8,
            vec![(3, 1, false, 0), (3, 2, false, 0), (3, 0, false, 0)],
        ),
    ] {
        let mut boundary = [0; 160];
        boundary[0] = 1;
        put64(&mut boundary, 8, cut);
        put32(&mut boundary, 16, 3);
        put32(&mut boundary, 20, events.len() as u32);
        records.push(boundary);
        for slot in 0..3 {
            let mut d = [0; 160];
            d[0] = 2;
            put64(&mut d, 8, cut);
            put32(&mut d, 16, slot);
            put32(&mut d, 20, 1);
            let values = if slot == 0 {
                d[1] = if cut == 0 {
                    2
                } else if cut == 12 || cut == start + 8 {
                    0
                } else {
                    1
                };
                d[2] = if cut == 8 {
                    9
                } else if d[1] == 1 {
                    73
                } else {
                    72
                };
                [
                    if cut == 0 { 0 } else { 4 },
                    u64::from(cut != 0),
                    0,
                    u64::from(cut >= 8 && cut < start),
                    if cut == 0 { 0 } else { 4 },
                    1,
                    if cut >= start {
                        start
                    } else if cut >= 8 {
                        8
                    } else {
                        cut
                    },
                    0,
                    0,
                    0,
                ]
            } else if cut < start {
                d[2] = 16;
                [0; 10]
            } else {
                d[1] = u8::from(cut < start + 8);
                d[2] = 120;
                [
                    if slot == 1 { 8 } else { 4 },
                    0,
                    0,
                    0,
                    0,
                    0,
                    start,
                    u64::from(slot == 1 && cut == start + 4),
                    0,
                    cut,
                ]
            };
            for (index, value) in values.into_iter().enumerate() {
                put64(&mut d, 24 + index * 8, value);
            }
            records.push(d);
        }
        for (code, target, flag, arg) in events {
            let mut e = [0; 160];
            e[0] = 3;
            e[1] = code;
            e[2] = u8::from(flag);
            put64(&mut e, 8, cut);
            put32(&mut e, 16, target);
            put32(&mut e, 20, 1);
            put64(&mut e, 24, arg);
            records.push(e);
        }
    }
    let mut result = vec![0; 40];
    result[..4].copy_from_slice(b"LSJ1");
    result[4..6].copy_from_slice(&1u16.to_le_bytes());
    result[6..8].copy_from_slice(&160u16.to_le_bytes());
    put32(&mut result, 8, records.len() as u32);
    for record in records {
        result.extend(record);
    }
    result
}
