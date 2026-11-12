use super::*;
use rt_runtime::Sample;

pub const CONTENT_FAMILY_RECORD_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContentFamilyEntry {
    pub target: LoopId,
    pub sync_source: LoopId,
    pub expected_revision: Revision,
    pub expected_length: u64,
    pub media: Option<media::MediaId>,
    pub restart: bool,
    pub fence_content: bool,
}

impl ContentFamilyEntry {
    pub fn encode(self, output: &mut [u8]) -> Result<(), Error> {
        if output.len() != CONTENT_FAMILY_RECORD_BYTES {
            return Err(Error::InvalidShape);
        }
        output.fill(0);
        for (at, value) in [
            (0, self.target.slot()),
            (4, self.target.generation()),
            (24, self.sync_source.slot()),
            (28, self.sync_source.generation()),
        ] {
            output[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        output[8..16].copy_from_slice(&self.expected_revision.0.to_le_bytes());
        output[16..24].copy_from_slice(&self.expected_length.to_le_bytes());
        if let Some(media) = self.media {
            for (index, value) in media.parts().into_iter().enumerate() {
                output[32 + index * 4..36 + index * 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        output[44] = u8::from(self.media.is_some())
            | (u8::from(self.restart) << 1)
            | (u8::from(self.fence_content) << 2);
        Ok(())
    }

    pub fn decode(input: &[u8]) -> Result<Self, Error> {
        if input.len() != CONTENT_FAMILY_RECORD_BYTES
            || input[44] & !7 != 0
            || input[45..].iter().any(|byte| *byte != 0)
        {
            return Err(Error::InvalidShape);
        }
        let word = |at| u32::from_le_bytes(input[at..at + 4].try_into().expect("sized record"));
        let media = if input[44] & 1 != 0 {
            Some(media::MediaId::from_parts([word(32), word(36), word(40)])?)
        } else {
            if input[32..44].iter().any(|byte| *byte != 0) {
                return Err(Error::InvalidShape);
            }
            None
        };
        if media.is_some() && input[44] & 4 == 0 {
            return Err(Error::InvalidShape);
        }
        Ok(Self {
            target: LoopId::from_parts(word(0), word(4)).map_err(|_| Error::InvalidId)?,
            sync_source: LoopId::from_parts(word(24), word(28)).map_err(|_| Error::InvalidId)?,
            expected_revision: Revision(u64::from_le_bytes(
                input[8..16].try_into().expect("sized record"),
            )),
            expected_length: u64::from_le_bytes(input[16..24].try_into().expect("sized record")),
            media,
            restart: input[44] & 2 != 0,
            fence_content: input[44] & 4 != 0,
        })
    }
}

#[derive(Clone, Copy)]
pub(super) struct Selection {
    entry: ContentFamilyEntry,
    retime: Option<crate::ContentRetime>,
}

pub(super) fn prepare(slots: usize, target: LoopId) -> Vec<Selection> {
    let mut entries = vec![
        Selection {
            entry: ContentFamilyEntry {
                target,
                sync_source: target,
                expected_revision: Revision(0),
                expected_length: 0,
                media: None,
                restart: false,
                fence_content: true,
            },
            retime: None
        };
        slots
    ];
    for entry in &mut entries {
        core::hint::black_box(entry);
    }
    entries.clear();
    entries
}

fn descendant(
    topology: &PreparedTopology,
    mut target: LoopId,
    root: LoopId,
) -> Result<bool, crate::Error> {
    for _ in 0..=topology.nodes().len() {
        if target == root {
            return Ok(true);
        }
        match topology.node(target)?.sync_source {
            Some(source) if source != target => target = source,
            _ => return Ok(false),
        }
    }
    Err(crate::Error::InvalidState)
}

impl<S: Sample> LoopPlanner<S> {
    pub(super) fn select_family(
        &mut self,
        root: LoopId,
        expected: Epoch,
        payload: &[u8],
        io: &mut PlannerResourceIo<'_, S>,
    ) -> Result<(), Failure> {
        if expected != self.resolver.epoch()
            || payload.is_empty()
            || payload.len() % CONTENT_FAMILY_RECORD_BYTES != 0
            || payload.len() / CONTENT_FAMILY_RECORD_BYTES > self.rows.len()
        {
            return Err(Error::InvalidShape.into());
        }
        let records = payload
            .chunks_exact(CONTENT_FAMILY_RECORD_BYTES)
            .position(|record| record.iter().all(|byte| *byte == 0))
            .unwrap_or(payload.len() / CONTENT_FAMILY_RECORD_BYTES);
        let end = records * CONTENT_FAMILY_RECORD_BYTES;
        if end == 0 || payload[end..].iter().any(|byte| *byte != 0) {
            return Err(Error::InvalidShape.into());
        }
        let payload = &payload[..end];
        self.family.clear();
        for record in payload.chunks_exact(CONTENT_FAMILY_RECORD_BYTES) {
            let entry = ContentFamilyEntry::decode(record)?;
            let row = self.row(entry.target)?;
            let node = self.resolver.topology().node(entry.target)?;
            if node.sync_source.unwrap_or(node.id) != entry.sync_source
                || self
                    .family
                    .iter()
                    .any(|selection| selection.entry.target == entry.target)
            {
                return Err(Error::InvalidId.into());
            }
            let state = self.resolver.state(entry.target)?;
            if entry.fence_content
                && (state.content_revision != entry.expected_revision
                    || state.content_length != entry.expected_length)
            {
                return Err(Error::InvalidState.into());
            }
            let retime = if let Some(media) = entry.media {
                if !descendant(self.resolver.topology(), entry.target, root)? {
                    return Err(Error::InvalidShape.into());
                }
                if row.active.is_some() || row.prepared.is_some() || state.writes() {
                    return Err(Error::Busy.into());
                }
                let view = self.ledger.view(media)?;
                Self::validate_media_shape(row, view)?;
                if view.description.owner != entry.target {
                    return Err(Error::InvalidId.into());
                }
                Some(crate::ContentRetime {
                    target: entry.target,
                    expected_revision: entry.expected_revision,
                    expected_length: entry.expected_length,
                    revision: view.description.revision,
                    length: view.description.frames,
                    restart: entry.restart,
                })
            } else {
                None
            };
            self.family.push(Selection { entry, retime });
        }
        if !self
            .family
            .iter()
            .any(|selection| selection.entry.target == root)
        {
            return Err(Error::InvalidShape.into());
        }
        for node in self.resolver.topology().nodes() {
            if descendant(self.resolver.topology(), node.id, root)?
                && !self
                    .family
                    .iter()
                    .any(|selection| selection.entry.target == node.id)
            {
                return Err(Error::InvalidShape.into());
            }
        }
        let changes = self.family.iter().filter_map(|selection| {
            selection.entry.media.map(|media| media::PlaybackChange {
                owner: selection.entry.target,
                media,
            })
        });
        let activation = self.ledger.prepare_playback_activation(io, changes)?;
        self.resolver
            .retime_content(self.family.iter().filter_map(|selection| selection.retime))?;
        activation.commit();
        for selection in &self.family {
            if let Some(media) = selection.entry.media {
                let row = self.rows[selection.entry.target.slot() as usize]
                    .as_mut()
                    .ok_or(Error::InvalidId)?;
                row.windows = self.ledger.view(media)?.windows.unwrap_or_default();
                row.length_override = None;
            }
        }
        Ok(())
    }
}
