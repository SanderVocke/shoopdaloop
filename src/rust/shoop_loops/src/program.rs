//! Control-side compilation of sparse, finite composite programs.

use crate::{Error, Limits, LoopId};

pub use shoop_loop_plan::{Mode, ProgramKind, Segment};
pub type Entry = shoop_loop_plan::Entry<LoopId>;
pub type Section = shoop_loop_plan::Section<LoopId>;
pub type ProgramDescription = shoop_loop_plan::Description<LoopId>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TargetTimeline<'a> {
    pub target: LoopId,
    segments: &'a [Segment],
}

impl<'a> TargetTimeline<'a> {
    pub fn segments(&self) -> &'a [Segment] {
        &self.segments
    }

    /// Lookup is logarithmic in prepared boundaries, not in timeline duration.
    pub fn at(&self, iteration: u64) -> Option<Segment> {
        let index = self
            .segments
            .partition_point(|segment| segment.start <= iteration);
        index
            .checked_sub(1)
            .and_then(|index| self.segments.get(index))
            .copied()
            .filter(|segment| iteration < segment.end)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TableRange {
    pub start: usize,
    pub len: usize,
}
impl TableRange {
    pub fn slice<T>(self, values: &[T]) -> &[T] {
        &values[self.start..self.start + self.len]
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TargetRecord {
    pub target: LoopId,
    pub segments: TableRange,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProgramRecord {
    pub source: LoopId,
    pub kind: ProgramKind,
    pub nominal_unit_frames: u64,
    pub iterations: u64,
    pub targets: TableRange,
    pub segments: TableRange,
}

/// Borrowed target metadata and segments; copying this view acquires no owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TargetTimelines<'a> {
    records: &'a [TargetRecord],
    segments: &'a [Segment],
}
impl<'a> TargetTimelines<'a> {
    pub fn len(&self) -> usize {
        self.records.len()
    }
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
    pub fn get(&self, index: usize) -> Option<TargetTimeline<'a>> {
        self.records.get(index).map(|record| TargetTimeline {
            target: record.target,
            segments: record.segments.slice(self.segments),
        })
    }
    pub fn iter(&self) -> TargetIter<'a> {
        TargetIter {
            records: self.records.iter(),
            segments: self.segments,
        }
    }
}
impl<'a> IntoIterator for TargetTimelines<'a> {
    type Item = TargetTimeline<'a>;
    type IntoIter = TargetIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
#[derive(Clone)]
pub struct TargetIter<'a> {
    records: core::slice::Iter<'a, TargetRecord>,
    segments: &'a [Segment],
}
impl<'a> Iterator for TargetIter<'a> {
    type Item = TargetTimeline<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        self.records.next().map(|record| TargetTimeline {
            target: record.target,
            segments: record.segments.slice(self.segments),
        })
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.records.size_hint()
    }
}
impl DoubleEndedIterator for TargetIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.records.next_back().map(|record| TargetTimeline {
            target: record.target,
            segments: record.segments.slice(self.segments),
        })
    }
}
impl ExactSizeIterator for TargetIter<'_> {}

/// Borrowed immutable program view backed by one topology's flat tables.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompiledProgram<'a> {
    pub source: LoopId,
    pub kind: ProgramKind,
    pub nominal_unit_frames: u64,
    pub iterations: u64,
    targets: TargetTimelines<'a>,
}
impl<'a> CompiledProgram<'a> {
    pub(crate) fn from_record(
        record: &ProgramRecord,
        targets: &'a [TargetRecord],
        segments: &'a [Segment],
    ) -> Self {
        Self {
            source: record.source,
            kind: record.kind,
            nominal_unit_frames: record.nominal_unit_frames,
            iterations: record.iterations,
            targets: TargetTimelines {
                records: record.targets.slice(targets),
                segments: record.segments.slice(segments),
            },
        }
    }
    pub fn targets(&self) -> TargetTimelines<'a> {
        self.targets
    }
    pub fn target(&self, id: LoopId) -> Option<TargetTimeline<'a>> {
        self.targets
            .records
            .binary_search_by_key(&id, |target| target.target)
            .ok()
            .and_then(|index| self.targets.get(index))
    }
    pub fn segment_count(&self) -> usize {
        self.targets.segments.len()
    }
    pub fn nominal_frames(&self) -> Result<u64, Error> {
        self.iterations
            .checked_mul(self.nominal_unit_frames)
            .ok_or(Error::Exhausted)
    }
}

type BuiltProgram = shoop_loop_plan::Compiled<LoopId>;

fn limits(value: Limits) -> shoop_loop_plan::Limits {
    shoop_loop_plan::Limits {
        program_entries: value.program_entries,
        program_segments: value.program_segments,
    }
}
impl From<shoop_loop_plan::Error> for Error {
    fn from(value: shoop_loop_plan::Error) -> Self {
        match value {
            shoop_loop_plan::Error::Capacity => Self::Capacity,
            shoop_loop_plan::Error::InvalidProgram => Self::InvalidProgram,
            shoop_loop_plan::Error::Exhausted => Self::Exhausted,
            shoop_loop_plan::Error::MissingTarget => Self::InvalidId,
        }
    }
}
pub(crate) fn expanded_entries(
    description: &ProgramDescription,
    capacity: Limits,
) -> Result<usize, Error> {
    shoop_loop_plan::expanded_entries(description, limits(capacity)).map_err(Error::from)
}
pub(crate) fn compile_program(
    description: &ProgramDescription,
    capacity: Limits,
    length: impl Fn(LoopId) -> Result<u64, Error>,
) -> Result<BuiltProgram, Error> {
    shoop_loop_plan::compile(description, limits(capacity), length)
}
