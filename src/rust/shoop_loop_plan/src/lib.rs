//! Control-side compilation of sparse programs, independent of engine ownership.
#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;
use alloc::{boxed::Box, collections::BTreeSet, vec::Vec};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Capacity,
    InvalidProgram,
    Exhausted,
    MissingTarget,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub program_entries: usize,
    pub program_segments: usize,
}
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum Mode {
    #[default]
    Stopped,
    Playing,
    Recording,
    PlayingDryThroughWet,
    RecordingDryIntoWet,
    Replacing,
}
impl Mode {
    pub fn records(self) -> bool {
        matches!(self, Self::Recording | Self::RecordingDryIntoWet)
    }
    pub fn playback_mode(self) -> bool {
        matches!(
            self,
            Self::Playing | Self::PlayingDryThroughWet | Self::RecordingDryIntoWet
        )
    }
    pub fn dry_playback(self) -> bool {
        matches!(self, Self::PlayingDryThroughWet | Self::RecordingDryIntoWet)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProgramKind {
    Regular,
    Explicit,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Entry<K> {
    pub target: K,
    pub delay: u64,
    pub duration: Option<u64>,
    pub repetitions: u32,
    pub mode: Option<Mode>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Section<K> {
    pub entries: Vec<Entry<K>>,
}
impl<K> Default for Section<K> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Description<K> {
    pub source: K,
    pub nominal_unit_frames: u64,
    pub sections: Vec<Section<K>>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Segment {
    pub start: u64,
    pub end: u64,
    pub mode: Mode,
    pub occurrence_start: u64,
    pub ordinal: u32,
}
pub struct BuiltTarget<K> {
    pub target: K,
    pub segments: Box<[Segment]>,
}
pub struct Compiled<K> {
    pub source: K,
    pub kind: ProgramKind,
    pub nominal_unit_frames: u64,
    pub iterations: u64,
    pub targets: Box<[BuiltTarget<K>]>,
    segment_count: usize,
}
impl<K> Compiled<K> {
    pub fn targets(&self) -> &[BuiltTarget<K>] {
        &self.targets
    }
    pub fn segment_count(&self) -> usize {
        self.segment_count
    }
    pub fn nominal_frames(&self) -> Result<u64, Error> {
        self.iterations
            .checked_mul(self.nominal_unit_frames)
            .ok_or(Error::Exhausted)
    }
}

pub fn expanded_entries<K: Copy + Ord>(
    description: &Description<K>,
    limits: Limits,
) -> Result<usize, Error> {
    if description.nominal_unit_frames == 0 || description.sections.len() > limits.program_entries {
        return Err(Error::InvalidProgram);
    }
    let mut count = 0usize;
    let mut raw = 0usize;
    for section in &description.sections {
        raw = raw
            .checked_add(section.entries.len())
            .ok_or(Error::Capacity)?;
        if raw > limits.program_entries {
            return Err(Error::Capacity);
        }
        for entry in &section.entries {
            if entry.repetitions == 0 || entry.duration == Some(0) {
                return Err(Error::InvalidProgram);
            }
            count = count
                .checked_add(entry.repetitions as usize)
                .ok_or(Error::Capacity)?;
            if count > limits.program_entries {
                return Err(Error::Capacity);
            }
        }
    }
    Ok(count)
}

#[derive(Clone, Copy)]
struct Span<K> {
    target: K,
    start: u64,
    end: u64,
    mode: Mode,
    ordinal: u32,
}

pub fn compile<K: Copy + Ord, E: From<Error>>(
    description: &Description<K>,
    limits: Limits,
    length: impl Fn(K) -> Result<u64, E>,
) -> Result<Compiled<K>, E> {
    let count = expanded_entries(description, limits)?;
    let mut explicit = false;
    let mut implicit = false;
    let mut spans = Vec::with_capacity(count);
    let mut base = 0u64;
    for section in &description.sections {
        let mut section_end = base;
        for entry in &section.entries {
            explicit |= entry.mode.is_some();
            implicit |= entry.mode.is_none();
            if explicit && implicit {
                return Err(Error::InvalidProgram.into());
            }
            let child_length = length(entry.target)?;
            let unit = description.nominal_unit_frames;
            let duration = entry
                .duration
                .unwrap_or_else(|| child_length.div_ceil(unit).max(1));
            let mut start = base.checked_add(entry.delay).ok_or(Error::Exhausted)?;
            for _ in 0..entry.repetitions {
                let end = start.checked_add(duration).ok_or(Error::Exhausted)?;
                spans.push(Span {
                    target: entry.target,
                    start,
                    end,
                    mode: entry.mode.unwrap_or(Mode::Playing),
                    ordinal: u32::try_from(spans.len()).map_err(|_| Error::Capacity)?,
                });
                start = end;
            }
            section_end = section_end.max(start);
        }
        base = section_end;
    }
    let mut targets = Vec::new();
    let mut segment_count = 0usize;
    for target in spans
        .iter()
        .map(|span| span.target)
        .collect::<BTreeSet<_>>()
    {
        let boundaries: Vec<_> = spans
            .iter()
            .filter(|span| span.target == target)
            .flat_map(|span| [span.start, span.end])
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut segments: Vec<Segment> = Vec::new();
        for range in boundaries.windows(2) {
            let Some(winner) = spans
                .iter()
                .filter(|span| {
                    span.target == target && span.start <= range[0] && range[0] < span.end
                })
                .max_by_key(|span| span.ordinal)
            else {
                continue;
            };
            if let Some(previous) = segments.last_mut() {
                if previous.end == range[0] && previous.mode == winner.mode {
                    previous.end = range[1];
                    continue;
                }
            }
            segment_count = segment_count.checked_add(1).ok_or(Error::Capacity)?;
            if segment_count > limits.program_segments {
                return Err(Error::Capacity.into());
            }
            segments.push(Segment {
                start: range[0],
                end: range[1],
                mode: winner.mode,
                occurrence_start: range[0],
                ordinal: winner.ordinal,
            });
        }
        targets.push(BuiltTarget {
            target,
            segments: segments.into_boxed_slice(),
        });
    }
    let program = Compiled {
        source: description.source,
        kind: if explicit {
            ProgramKind::Explicit
        } else {
            ProgramKind::Regular
        },
        nominal_unit_frames: description.nominal_unit_frames,
        iterations: base,
        targets: targets.into_boxed_slice(),
        segment_count,
    };
    program.nominal_frames()?;
    Ok(program)
}
