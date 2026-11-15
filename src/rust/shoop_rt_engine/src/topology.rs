//! Allocation-free LTP2 timing images, including sparse composite programs.

use crate::model::OwnedSpan;
use alloc::boxed::Box;
use alloc::vec;
use shoop_loops::{Limits, LoopId};
use shoop_rt_protocol::{CompositeDescription, CompositeKind};

pub(crate) const RECORD_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TopologyLoop {
    pub id: LoopId,
    pub sync: LoopId,
    pub length: u64,
    pub program: Option<CompositeDescription>,
}

pub(crate) struct Encoder {
    limits: Limits,
    loops: Box<[Option<TopologyLoop>]>,
    order: Box<[usize]>,
    emitted: Box<[bool]>,
}
fn put32(bytes: &mut [u8], at: usize, value: usize) {
    bytes[at..at + 4].copy_from_slice(&(value as u32).to_le_bytes());
}
fn put64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn put_id(bytes: &mut [u8], at: usize, id: LoopId) {
    put32(bytes, at, id.slot() as usize);
    put32(bytes, at + 4, id.generation() as usize);
}
fn runtime_id(id: shoop_rt_protocol::LoopSlotId) -> Result<LoopId, rt_runtime::Error> {
    LoopId::from_parts(id.0.index, id.0.generation).map_err(|_| rt_runtime::Error::InvalidId)
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
        RECORD_BYTES
            * (1 + 3 * self.limits.loop_slots
                + self.limits.program_entries
                + self.limits.program_segments)
    }
    pub(crate) fn encode(
        &mut self,
        source: impl Fn(usize) -> Option<TopologyLoop>,
        spans: &[Option<OwnedSpan>],
        output: &mut [u8],
    ) -> Result<usize, rt_runtime::Error> {
        use rt_runtime::Error;
        let slots = self.limits.loop_slots;
        let (mut count, mut programs, mut targets, mut segments) = (0, 0, 0, 0);
        for slot in 0..slots {
            self.loops[slot] = source(slot);
            self.emitted[slot] = false;
            if let Some(node) = self.loops[slot] {
                count += 1;
                if let Some(program) = node.program {
                    if runtime_id(program.loop_id)? != node.id {
                        return Err(Error::InvalidId);
                    }
                    programs += 1;
                    let mut previous = None;
                    for owned in spans
                        .iter()
                        .flatten()
                        .filter(|s| s.source == program.loop_id)
                    {
                        if previous != Some(owned.span.target) {
                            targets += 1;
                            previous = Some(owned.span.target);
                        }
                        segments += 1;
                    }
                }
            }
        }
        let relations = count + targets;
        if relations > self.limits.relations
            || targets > self.limits.program_entries
            || segments > self.limits.program_segments
        {
            return Err(Error::Capacity);
        }
        for node in self.loops.iter().flatten() {
            if self
                .loops
                .get(node.sync.slot() as usize)
                .and_then(|n| *n)
                .is_none_or(|n| n.id != node.sync)
            {
                return Err(Error::InvalidId);
            }
        }
        for owned in spans.iter().flatten() {
            let source = runtime_id(owned.source)?;
            let target = runtime_id(owned.span.target)?;
            if self
                .loops
                .get(source.slot() as usize)
                .and_then(|n| *n)
                .is_none_or(|n| n.id != source || n.program.is_none())
                || self
                    .loops
                    .get(target.slot() as usize)
                    .and_then(|n| *n)
                    .is_none_or(|n| n.id != target)
            {
                return Err(Error::InvalidId);
            }
        }
        let mut ordered = 0;
        while ordered < count {
            let ready = (0..slots).find(|&slot| {
                !self.emitted[slot]
                    && self.loops[slot].is_some_and(|node| {
                        let sync = node.sync.slot() as usize;
                        (sync == slot || self.emitted[sync])
                            && spans
                                .iter()
                                .flatten()
                                .filter(|s| s.span.target.0.index as usize == slot)
                                .all(|s| self.emitted[s.source.0.index as usize])
                    })
            });
            let Some(slot) = ready else {
                return Err(Error::Cycle);
            };
            self.emitted[slot] = true;
            self.order[ordered] = slot;
            ordered += 1;
        }
        let counts = [count, 0, programs, targets, segments, count];
        let records = counts.iter().sum();
        let output = output
            .get_mut(..RECORD_BYTES * (1 + records))
            .ok_or(Error::Capacity)?;
        output.fill(0);
        let (header, body) = output.split_at_mut(RECORD_BYTES);
        header[..4].copy_from_slice(b"LTP2");
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
        for (index, value) in counts.into_iter().enumerate() {
            put32(header, 32 + 4 * index, value);
        }
        put32(header, 60, relations);
        let mut records = body.chunks_exact_mut(RECORD_BYTES);
        for node in self.loops.iter().flatten() {
            let record = records.next().expect("sized image");
            put_id(record, 0, node.id);
            record[8] = u8::from(node.program.is_some());
            record[9] = 1;
            let length = match node.program {
                Some(p) => p
                    .unit_frames
                    .checked_mul(p.iterations)
                    .ok_or(Error::Capacity)?,
                None => node.length,
            };
            put64(record, 16, length);
            put_id(record, 24, node.sync);
        }
        let (mut target_base, mut segment_base) = (0, 0);
        for node in self.loops.iter().flatten().filter(|n| n.program.is_some()) {
            let program = node.program.unwrap();
            let record = records.next().expect("sized image");
            let mut previous = None;
            let (mut targets, mut segments) = (0, 0);
            for owned in spans
                .iter()
                .flatten()
                .filter(|s| s.source == program.loop_id)
            {
                if previous != Some(owned.span.target) {
                    targets += 1;
                    previous = Some(owned.span.target);
                }
                segments += 1;
            }
            put_id(record, 0, node.id);
            record[8] = u8::from(program.kind == CompositeKind::Explicit);
            put64(record, 16, program.unit_frames);
            put64(record, 24, program.iterations);
            put32(record, 32, target_base);
            put32(record, 36, targets);
            put32(record, 40, segment_base);
            put32(record, 44, segments);
            target_base += targets;
            segment_base += segments;
        }
        for node in self.loops.iter().flatten().filter(|n| n.program.is_some()) {
            let program = node.program.unwrap();
            let mut index = 0;
            let mut iter = spans
                .iter()
                .flatten()
                .filter(|s| s.source == program.loop_id)
                .peekable();
            while let Some(first) = iter.next() {
                let start = index;
                index += 1;
                while iter
                    .peek()
                    .is_some_and(|s| s.span.target == first.span.target)
                {
                    iter.next();
                    index += 1;
                }
                let record = records.next().expect("sized image");
                put_id(record, 0, runtime_id(first.span.target)?);
                put32(record, 8, start);
                put32(record, 12, index - start);
            }
        }
        for node in self.loops.iter().flatten().filter(|n| n.program.is_some()) {
            let program = node.program.unwrap();
            for owned in spans
                .iter()
                .flatten()
                .filter(|s| s.source == program.loop_id)
            {
                let span = owned.span;
                let record = records.next().expect("sized image");
                put64(record, 0, span.start);
                put64(record, 8, span.end);
                put64(record, 16, span.occurrence_start);
                put32(record, 24, span.ordinal as usize);
                record[28] = match span.mode {
                    shoop_rt_protocol::LoopMode::Stopped => 0,
                    shoop_rt_protocol::LoopMode::Playing => 1,
                    shoop_rt_protocol::LoopMode::Recording => 2,
                    shoop_rt_protocol::LoopMode::PlayingDryThroughWet => 3,
                    shoop_rt_protocol::LoopMode::RecordingDryIntoWet => 4,
                    shoop_rt_protocol::LoopMode::Replacing => 5,
                };
            }
        }
        for &slot in &self.order[..count] {
            put_id(
                records.next().expect("sized image"),
                0,
                self.loops[slot].unwrap().id,
            );
        }
        Ok(counts.iter().sum())
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
                        .map(|&(id, sync)| TopologyLoop {
                            id,
                            sync,
                            length: 0,
                            program: None,
                        })
                },
                &[],
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
                    length: 0,
                    program: None,
                }),
                1 => Some(TopologyLoop {
                    id: id(1, 1),
                    sync: id(0, 1),
                    length: 0,
                    program: None,
                }),
                _ => None,
            },
            &[],
            &mut image,
        );
        assert_eq!(cyclic, Err(rt_runtime::Error::Cycle));
    }
}
