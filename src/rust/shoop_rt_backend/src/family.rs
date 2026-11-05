//! Control-side preparation of a fenced, atomic content family.
use crate::{
    persistence::{Job, Work},
    prepared::{Upload, MAX_MEDIA_BYTES, MAX_MIDI_EVENTS},
    EngineClock, RtBackend,
};
use alloc::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
    vec::Vec,
};
use anyhow::{anyhow, bail, Result};
use shoop_backend::*;
use shoop_rt_protocol::{ContentFamilyEdit, LoopMode, Request};
use shoop_rt_transport::carrier::Carrier;

impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    pub(crate) fn import_family(
        &mut self,
        update: &BackendLoopFamilyUpdate,
    ) -> Result<BackendAsyncResult<()>> {
        if let Some(replacement) = &self.replacement {
            if replacement.job.family != Some(update.root) {
                bail!("another persistence operation is active");
            }
            return self.persistence_progress().map(|result| match result {
                BackendAsyncResult::Ready(_) => BackendAsyncResult::Ready(()),
                BackendAsyncResult::Pending(progress) => BackendAsyncResult::Pending(progress),
            });
        }
        self.pump();
        self.ready()?;
        if self.requests.timing_pending() {
            bail!("a timing edit is pending");
        }
        if update.sample_rate != self.driver.map_or(0, |driver| driver.sample_rate)
            || update.quantum_frames != u32::from(self.limits().quantum)
            || update.members.is_empty()
            || update.members.len() > self.limits().loops as usize
        {
            bail!("stale or invalid family target format or size");
        }
        let root = self
            .mirror
            .live_loop(update.root)
            .ok_or_else(|| anyhow!("unknown family root"))?
            .slot;
        let mut members = BTreeSet::new();
        let mut content = BTreeMap::new();
        for member in &update.members {
            let entry = self
                .mirror
                .live_loop(member.loop_id)
                .ok_or_else(|| anyhow!("unknown family member"))?;
            if !members.insert(member.loop_id)
                || entry.sync.unwrap_or(member.loop_id)
                    != member.sync_source.unwrap_or(member.loop_id)
                || (member.fence_content
                    && (entry.content != member.expected_revision
                        || entry.view.length != u64::from(member.expected_frames)))
                || (member.content.is_some() && !member.fence_content)
                || (member.restart && member.content.is_none())
            {
                bail!("family content or dependency changed during preparation");
            }
            if let Some(image) = &member.content {
                if entry.view.recording_armed
                    || entry.view.mode == LoopMode::Recording
                    || entry.view.next_mode == Some(LoopMode::Recording)
                {
                    bail!("stop affected recording before replacing family content");
                }
                let track = &self.mirror.tracks[&entry
                    .track
                    .ok_or_else(|| anyhow!("composites have no media channels"))?];
                if image.source_id != member.loop_id.raw()
                    || image.length % update.quantum_frames != 0
                    || image.audio.len() != track.audio_channels as usize
                    || image.midi.len() != usize::from(track.midi)
                {
                    bail!("family content has invalid identity, channels or length");
                }
                content.insert(member.loop_id, Arc::new(image.clone()));
            }
        }
        if !content.contains_key(&update.root) {
            bail!("family must replace its root");
        }
        let retained = self
            .canonical_imports
            .iter()
            .filter(|id| !content.contains_key(id))
            .try_fold(0u64, |size, id| {
                let proof = self
                    .prepared_content
                    .get(id)
                    .filter(|proof| proof.matches(&self.canonical_content[id]))
                    .ok_or_else(|| anyhow!("retained content lacks a validation proof"))?;
                size.checked_add(proof.bytes())
                    .ok_or_else(|| anyhow!("retained media size overflow"))
            })?;
        let events = content
            .values()
            .chain(
                self.canonical_imports
                    .iter()
                    .filter(|id| !content.contains_key(id))
                    .map(|id| &self.canonical_content[id]),
            )
            .try_fold(0usize, |count, image| {
                image.midi.iter().try_fold(count, |count, channel| {
                    count
                        .checked_add(channel.events.len())
                        .ok_or_else(|| anyhow!("MIDI event count overflow"))
                })
            })?;
        if events > MAX_MIDI_EVENTS {
            bail!("retained canonical MIDI exceeds the one-million-event budget");
        }
        let id = self.family_token()?;
        let mut work = VecDeque::new();
        let mut uploads = Vec::new();
        work.push_back(Work::Request(Request::ContentFamilyBegin {
            id,
            root,
            count: update.members.len() as u32,
            sample_rate: update.sample_rate,
            quantum: self.limits().quantum,
        }));
        for member in &update.members {
            let mut pair = [None; 2];
            if let Some(image) = content.get(&member.loop_id).filter(|image| {
                image.length > 0
                    || image
                        .audio
                        .iter()
                        .any(|channel| !channel.samples.is_empty())
                    || image.midi.iter().any(|channel| channel.length > 0)
            }) {
                for (index, midi) in [false, true].into_iter().enumerate() {
                    let token = self.family_token()?;
                    let upload = Upload::new(
                        token,
                        midi,
                        Arc::clone(image),
                        usize::from(self.limits().strip_audio_channels),
                    )?;
                    uploads.push(token);
                    pair[index] = Some(token);
                    work.push_back(Work::Upload {
                        upload,
                        begun: false,
                    });
                    work.push_back(Work::Request(Request::ContentWindow {
                        id: token,
                        frames: u64::from(image.length),
                        windows: crate::prepared::windows(&image),
                    }));
                }
            }
            let entry = &self.mirror.loops[&member.loop_id];
            work.push_back(Work::Request(Request::ContentFamilyEdit {
                id,
                edit: ContentFamilyEdit {
                    loop_id: entry.slot,
                    sync_source: member
                        .sync_source
                        .map(|id| {
                            self.mirror
                                .live_loop(id)
                                .map(|entry| entry.slot)
                                .ok_or_else(|| anyhow!("unknown family sync source"))
                        })
                        .transpose()?
                        .unwrap_or(entry.slot),
                    expected_revision: entry.rt_content_revision,
                    expected_frames: u64::from(member.expected_frames),
                    frames: content
                        .get(&member.loop_id)
                        .map_or(u64::from(member.expected_frames), |image| {
                            u64::from(image.length)
                        }),
                    audio: pair[0],
                    midi: pair[1],
                    replace: member.content.is_some(),
                    restart: member.restart,
                    fence_content: member.fence_content,
                },
            }));
        }
        work.push_back(Work::Request(Request::ContentFamilyCommit { id }));
        let mut job = Job::new(
            id,
            work,
            uploads,
            false,
            content.values().cloned(),
            usize::from(self.limits().strip_audio_channels),
            MAX_MEDIA_BYTES.saturating_sub(retained),
        )?;
        job.family = Some(update.root);
        self.replacement = Some(crate::Replacement {
            mapping: Default::default(),
            candidate: None,
            job,
            content,
            connection_failures: Vec::new(),
            loop_target: Some(update.root),
            published: false,
        });
        self.import_family(update)
    }

    fn family_token(&mut self) -> Result<u64> {
        let id = self.next_persistence;
        self.next_persistence = id
            .checked_add(1)
            .ok_or_else(|| anyhow!("persistence identity exhausted"))?;
        Ok(id)
    }
}
