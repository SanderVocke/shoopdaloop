use super::*;
use crate::{CompositeCommand, CompositeDescription, CompositeKind, CompositeSpan};

tags!(CompositeKind { 0 => CompositeKind::Regular, 1 => CompositeKind::Explicit });

impl Wire for CompositeDescription {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        w.u64(self.id)?;
        self.loop_id.put(w)?;
        self.previous.put(w)?;
        w.u64(self.expected_version)?;
        w.u64(self.version)?;
        self.kind.put(w)?;
        w.u64(self.unit_frames)?;
        w.u64(self.iterations)?;
        w.u32(self.entries)?;
        w.u32(self.segments)?;
        w.u32(self.sample_rate)?;
        w.u32(self.quantum)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            id: r.u64()?,
            loop_id: LoopSlotId::get(r)?,
            previous: Option::get(r)?,
            expected_version: r.u64()?,
            version: r.u64()?,
            kind: CompositeKind::get(r)?,
            unit_frames: r.u64()?,
            iterations: r.u64()?,
            entries: r.u32()?,
            segments: r.u32()?,
            sample_rate: r.u32()?,
            quantum: r.u32()?,
        })
    }
}

impl Wire for CompositeSpan {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.target.put(w)?;
        w.u64(self.start)?;
        w.u64(self.end)?;
        w.u64(self.occurrence_start)?;
        w.u32(self.ordinal)?;
        self.mode.put(w)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            target: LoopSlotId::get(r)?,
            start: r.u64()?,
            end: r.u64()?,
            occurrence_start: r.u64()?,
            ordinal: r.u32()?,
            mode: LoopMode::get(r)?,
        })
    }
}

impl Wire for CompositeCommand {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        match self {
            Self::Begin(description) => {
                w.u8(0)?;
                description.put(w)
            }
            Self::Span { id, index, span } => {
                w.u8(1)?;
                w.u64(*id)?;
                w.u32(*index)?;
                span.put(w)
            }
            Self::Commit { id } => {
                w.u8(2)?;
                w.u64(*id)
            }
            Self::Cancel { id } => {
                w.u8(3)?;
                w.u64(*id)
            }
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(match r.u8()? {
            0 => Self::Begin(CompositeDescription::get(r)?),
            1 => Self::Span {
                id: r.u64()?,
                index: r.u32()?,
                span: CompositeSpan::get(r)?,
            },
            2 => Self::Commit { id: r.u64()? },
            3 => Self::Cancel { id: r.u64()? },
            _ => return Err(CodecError::Invalid),
        })
    }
}
