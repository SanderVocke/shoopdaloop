//! Allocation-free LTP1 timing images of the engine's loops.
//!
//! The engine's topology has only primitive loops. Every loop has exactly one
//! sync relation: to its sync source, or to itself when it has none, so its own
//! wraps count delayed transitions. Sync relations between different loops form
//! a forest; each loop is its own component, ordered by repeatedly taking the
//! lowest ready identity.

use shoop_loops::{Limits, LoopId};

pub(crate) const RECORD_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TopologyLoop {
    pub id: LoopId,
    pub sync: LoopId,
}

/// Prepared scratch for encoding images of up to `loops` loops.
pub(crate) struct Encoder {
    limits: Limits,
    loops: Box<[Option<TopologyLoop>]>,
    order: Box<[usize]>,
    emitted: Box<[bool]>,
}

fn put32(bytes: &mut [u8], at: usize, value: usize) {
    bytes[at..at + 4].copy_from_slice(&(value as u32).to_le_bytes());
}
fn put_id(bytes: &mut [u8], at: usize, id: LoopId) {
    put32(bytes, at, id.slot() as usize);
    put32(bytes, at + 4, id.generation() as usize);
}

impl Encoder {
    pub(crate) fn new(limits: Limits) -> Self {
        let slots = limits.loop_slots;
        Self {
            limits,
            loops: vec![None; slots].into_boxed_slice(),
            order: vec![0; slots].into_boxed_slice(),
            emitted: vec![false; slots].into_boxed_slice(),
        }
    }
    pub(crate) fn image_bytes(&self) -> usize {
        RECORD_BYTES * (1 + 3 * self.limits.loop_slots)
    }

    /// Encode the loops yielded by `source` (by slot) into `output` and return the
    /// number of body records. The header occupies the first record.
    pub(crate) fn encode(
        &mut self,
        source: impl Fn(usize) -> Option<TopologyLoop>,
        output: &mut [u8],
    ) -> Result<usize, rt_runtime::Error> {
        let slots = self.limits.loop_slots;
        let mut count = 0;
        for slot in 0..slots {
            self.loops[slot] = source(slot);
            self.emitted[slot] = false;
            count += usize::from(self.loops[slot].is_some());
        }
        let mut ordered = 0;
        while ordered < count {
            let ready = (0..slots).find(|&slot| {
                !self.emitted[slot]
                    && self.loops[slot].is_some_and(|node| {
                        let source = node.sync.slot() as usize;
                        source == slot || self.emitted.get(source).copied().unwrap_or(false)
                    })
            });
            let Some(slot) = ready else {
                return Err(rt_runtime::Error::Cycle);
            };
            self.emitted[slot] = true;
            self.order[ordered] = slot;
            ordered += 1;
        }
        let records = 3 * count;
        let output = output
            .get_mut(..RECORD_BYTES * (1 + records))
            .ok_or(rt_runtime::Error::Capacity)?;
        output.fill(0);
        let (header, body) = output.split_at_mut(RECORD_BYTES);
        header[..4].copy_from_slice(b"LTP1");
        put32(header, 4, records);
        for (index, value) in [
            self.limits.loop_slots,
            self.limits.relations,
            self.limits.program_entries,
            self.limits.program_segments,
            self.limits.pending_requests,
            self.limits.journal_events,
        ]
        .into_iter()
        .enumerate()
        {
            put32(header, 8 + 4 * index, value);
        }
        for (index, value) in [count, 0, 0, 0, 0, count, count].into_iter().enumerate() {
            put32(header, 32 + 4 * index, value);
        }
        put32(header, 60, count);
        let mut records = body.chunks_exact_mut(RECORD_BYTES);
        for node in self.loops.iter().flatten() {
            let record = records.next().expect("sized image");
            put_id(record, 0, node.id);
            record[9] = 1;
            put_id(record, 24, node.sync);
        }
        for index in 0..count {
            let record = records.next().expect("sized image");
            put32(record, 0, index);
            put32(record, 4, 1);
        }
        for &slot in &self.order[..count] {
            let record = records.next().expect("sized image");
            put_id(record, 0, self.loops[slot].expect("ordered loop").id);
        }
        Ok(3 * count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_loops::{compile, LoopKind, LoopSpec, TopologyDescription};
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test]
    fn images_match_the_loop_library_compiler() {
        let limits = Limits {
            loop_slots: 6,
            relations: 6,
            program_entries: 1,
            program_segments: 1,
            pending_requests: 9,
            journal_events: 0,
        };
        let id = |slot, generation| LoopId::from_parts(slot, generation).unwrap();
        let loops = [
            (id(0, 2), id(3, 1)),
            (id(1, 1), id(1, 1)),
            (id(3, 1), id(5, 4)),
            (id(4, 7), id(1, 1)),
            (id(5, 4), id(5, 4)),
        ];
        let compiled = compile(
            limits,
            &TopologyDescription {
                loops: loops
                    .iter()
                    .map(|&(id, sync)| LoopSpec {
                        id,
                        kind: LoopKind::Primitive,
                        length: 0,
                        sync_source: Some(sync),
                    })
                    .collect(),
                ..TopologyDescription::default()
            },
        )
        .unwrap();
        let mut encoder = Encoder::new(limits);
        let mut image = vec![0; encoder.image_bytes()];
        let records = encoder
            .encode(
                |slot| {
                    loops
                        .iter()
                        .find(|(id, _)| id.slot() as usize == slot)
                        .map(|&(id, sync)| TopologyLoop { id, sync })
                },
                &mut image,
            )
            .unwrap();
        assert_eq!(records, compiled.encoded_records());
        let mut expected = [0; RECORD_BYTES];
        compiled.encode_header(&mut expected);
        assert_eq!(&image[..RECORD_BYTES], &expected);
        for record in 0..records {
            compiled.encode_record(record, &mut expected).unwrap();
            let start = RECORD_BYTES * (record + 1);
            assert_eq!(
                &image[start..start + RECORD_BYTES],
                &expected,
                "record {record}"
            );
        }
        let cyclic = encoder.encode(
            |slot| match slot {
                0 => Some(TopologyLoop {
                    id: id(0, 1),
                    sync: id(1, 1),
                }),
                1 => Some(TopologyLoop {
                    id: id(1, 1),
                    sync: id(0, 1),
                }),
                _ => None,
            },
            &mut image,
        );
        assert_eq!(cyclic, Err(rt_runtime::Error::Cycle));
    }
}
