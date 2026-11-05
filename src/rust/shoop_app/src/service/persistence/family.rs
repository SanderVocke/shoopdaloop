use super::*;
use alloc::collections::BTreeSet;

impl Service {
    fn family_source(&self, id: LoopId) -> Result<Option<LoopId>> {
        let entry = self
            .loops
            .get(&id)
            .ok_or_else(|| anyhow!("stale family member"))?;
        let state = self
            .observed
            .loops
            .get(&entry.backend)
            .ok_or_else(|| anyhow!("unconfirmed family member"))?;
        state
            .sync_source
            .map(|source| {
                self.loops
                    .iter()
                    .find_map(|(id, entry)| (entry.backend == source).then_some(*id))
                    .ok_or_else(|| anyhow!("stale family sync source"))
            })
            .transpose()
    }

    fn family_descendant(&self, mut id: LoopId, root: LoopId) -> Result<bool> {
        for _ in 0..=self.loops.len() {
            if id == root {
                return Ok(true);
            }
            match self.family_source(id)? {
                Some(source) if source != id => id = source,
                _ => return Ok(false),
            }
        }
        Err(anyhow!("cyclic sync dependencies"))
    }

    pub(super) fn prepare_family(&self, image: &image::Image) -> Result<BackendLoopFamilyUpdate> {
        let image::Manifest::FamilyUpdate {
            format,
            root,
            members,
        } = &image.manifest
        else {
            return Err(anyhow!("expected family image"));
        };
        if self.observed.status.sample_rate != format.sample_rate
            || self.observed.status.quantum_frames != format.quantum_frames
        {
            return Err(anyhow!("engine format changed during family preparation"));
        }
        let ids = members
            .iter()
            .map(|member| member.loop_id)
            .collect::<BTreeSet<_>>();
        for id in self.loops.keys() {
            if self.family_descendant(*id, *root)? && !ids.contains(id) {
                return Err(anyhow!("sync family changed during preparation"));
            }
        }
        let mut output = Vec::new();
        for member in members {
            let entry = self
                .loops
                .get(&member.loop_id)
                .ok_or_else(|| anyhow!("stale family member"))?;
            let state = &self.observed.loops[&entry.backend];
            if self
                .family_source(member.loop_id)?
                .unwrap_or(member.loop_id)
                != member.sync_source.unwrap_or(member.loop_id)
                || (member.fence_content
                    && (entry.view.content_revision != member.expected_revision
                        || entry.view.length_frames != member.expected_frames))
                || (!self.family_descendant(member.loop_id, *root)?
                    && (!self.family_descendant(*root, member.loop_id)?
                        || member.content.is_some()))
            {
                return Err(anyhow!(
                    "family content or dependency changed during preparation"
                ));
            }
            if member.content.is_some()
                && (state.recording_armed
                    || matches!(
                        state.mode,
                        BackendLoopMode::Recording | BackendLoopMode::RecordingDryIntoWet
                    )
                    || matches!(
                        state.next_mode,
                        Some(BackendLoopMode::Recording | BackendLoopMode::RecordingDryIntoWet)
                    ))
            {
                return Err(anyhow!("stop affected recording before importing"));
            }
            let content = member
                .content
                .as_ref()
                .map(|content| {
                    let mut output = mapping::backend_loop(image, content)?;
                    output.source_id = entry.backend.raw();
                    Ok::<_, anyhow::Error>(output)
                })
                .transpose()?;
            output.push(BackendLoopFamilyMember {
                loop_id: entry.backend,
                sync_source: member
                    .sync_source
                    .map(|source| {
                        self.loops
                            .get(&source)
                            .map(|entry| entry.backend)
                            .ok_or_else(|| anyhow!("stale sync source"))
                    })
                    .transpose()?,
                expected_revision: state.content_revision,
                expected_frames: u32::try_from(member.expected_frames)?,
                fence_content: member.fence_content,
                restart: member.restart,
                content,
            });
        }
        Ok(BackendLoopFamilyUpdate {
            root: self
                .loops
                .get(root)
                .ok_or_else(|| anyhow!("stale family root"))?
                .backend,
            sample_rate: format.sample_rate,
            quantum_frames: format.quantum_frames,
            members: output,
        })
    }

    pub(super) fn publish_family_annotations(
        &mut self,
        image: &image::Image,
        observed: &BackendSnapshot,
    ) {
        let image::Manifest::FamilyUpdate { members, .. } = &image.manifest else {
            return;
        };
        for member in members {
            let Some(content) = &member.content else {
                continue;
            };
            let revision = observed.loops[&self.loops[&member.loop_id].backend].content_revision;
            for (index, channel) in content.audio.iter().enumerate() {
                self.channel_annotations.insert(
                    (member.loop_id, false, index as u32),
                    (revision, channel.recording_started_at.clone()),
                );
            }
            for (index, channel) in content.midi.iter().enumerate() {
                self.channel_annotations.insert(
                    (member.loop_id, true, index as u32),
                    (revision, channel.channel.recording_started_at.clone()),
                );
            }
        }
    }
}
