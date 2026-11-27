use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotFormat {
    epoch: u32,
    quantum: u32,
    rows: usize,
}
impl SnapshotFormat {
    pub fn new(epoch: u32, quantum: u32, rows: usize) -> Result<Self, CodecError> {
        if epoch == 0 {
            return Err(CodecError::Identity);
        }
        if quantum == 0 || quantum > 128 || !(1..=SEMANTIC_LOOP_LIMIT as usize).contains(&rows) {
            return Err(CodecError::Geometry);
        }
        Ok(Self {
            epoch,
            quantum,
            rows,
        })
    }
    pub fn packet_words(self) -> usize {
        HEADER_WORDS + self.rows * ROW_WORDS
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotPhase {
    Initial,
    Settled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotHeader {
    pub epoch: u32,
    pub phase: SnapshotPhase,
    pub quantum: u32,
    pub sequence: u64,
    pub frame: u64,
    pub revision: u64,
    pub rows: usize,
    pub skipped: u64,
}
impl SnapshotHeader {
    pub fn decode(words: &[u32], expected: SnapshotFormat) -> Result<Self, CodecError> {
        if words.len() != HEADER_WORDS {
            return Err(CodecError::Length);
        }
        if words[0] != 0x5346_5331
            || words[1] & 0xffff != SEMANTIC_VERSION
            || words[14] != 0
            || words[15] != 0
            || join(&words[4..6]) == 0
        {
            return Err(CodecError::Format);
        }
        if words[2] != expected.epoch {
            return Err(CodecError::Identity);
        }
        if words[3] != expected.quantum
            || words[10] as usize != expected.rows
            || words[11] != SEMANTIC_ROW_WORDS
        {
            return Err(CodecError::Geometry);
        }
        let phase = match words[1] >> 16 {
            0 => SnapshotPhase::Initial,
            1 => SnapshotPhase::Settled,
            _ => return Err(CodecError::Format),
        };
        let header = Self {
            epoch: words[2],
            phase,
            quantum: words[3],
            sequence: join(&words[4..6]),
            frame: join(&words[6..8]),
            revision: join(&words[8..10]),
            rows: expected.rows,
            skipped: join(&words[12..14]),
        };
        if phase == SnapshotPhase::Initial
            && (header.sequence != 1
                || header.frame != 0
                || header.revision != 0
                || header.skipped != 0)
        {
            return Err(CodecError::Format);
        }
        Ok(header)
    }
}

/// Raw physical observations; graph membership comes only from matching immutable bindings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationRow {
    pub raw: [u64; 16],
    pub gain_bits: u32,
    pub failed_plans: u32,
    pub last_pulse: u64,
    pub pulse_valid: bool,
}

#[derive(Debug)]
pub struct Snapshot<'a> {
    header: SnapshotHeader,
    words: &'a [u32],
}
impl<'a> Snapshot<'a> {
    pub fn decode(words: &'a [u32], expected: SnapshotFormat) -> Result<Self, CodecError> {
        if words.len() != expected.packet_words() {
            return Err(CodecError::Length);
        }
        let header = SnapshotHeader::decode(&words[..HEADER_WORDS], expected)?;
        if words[HEADER_WORDS..]
            .chunks_exact(ROW_WORDS)
            .any(|row| join(&row[36..38]) > 1)
        {
            return Err(CodecError::Format);
        }
        Ok(Self { header, words })
    }
    pub fn header(&self) -> SnapshotHeader {
        self.header
    }
    pub fn physical_row(&self, slot: usize) -> Result<ObservationRow, CodecError> {
        if slot >= self.header.rows {
            return Err(CodecError::Binding);
        }
        let offset = HEADER_WORDS + slot * ROW_WORDS;
        let row = &self.words[offset..offset + ROW_WORDS];
        Ok(ObservationRow {
            raw: core::array::from_fn(|index| join(&row[index * 2..index * 2 + 2])),
            gain_bits: row[32],
            failed_plans: row[33],
            last_pulse: join(&row[34..36]),
            pulse_valid: row[36] != 0,
        })
    }
}
