//! A loop update stages both media while retaining the unchanged canonical channels.
use alloc::vec::Vec;

use crate::{
    persistence::{Job, Work},
    prepared::Upload,
    EngineClock, RtBackend,
};
use alloc::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};
use anyhow::{anyhow, bail, Result};
use shoop_backend::*;
use shoop_rt_protocol::Request;
use shoop_rt_transport::carrier::Carrier;
impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    pub(crate) fn import_content(
        &mut self,
        loop_id: BackendLoopId,
        update: &BackendLoopContentUpdate,
    ) -> Result<BackendAsyncResult<()>> {
        if let Some(replacement) = &self.replacement {
            if replacement.job.family.is_some()
                || (replacement.loop_target != Some(loop_id) && !replacement.job.finished)
            {
                bail!("another persistence operation is active");
            }
            return self.persistence_progress().map(|result| match result {
                BackendAsyncResult::Ready(_) => BackendAsyncResult::Ready(()),
                BackendAsyncResult::Pending(progress) => BackendAsyncResult::Pending(progress),
            });
        }
        self.ready()?;
        if self.requests.timing_pending() {
            bail!("a timing edit is pending");
        }
        if self.mirror.live_loop(loop_id).is_none() {
            bail!("unknown loop {loop_id:?}");
        }
        if !self.canonical_content.contains_key(&loop_id) {
            if let BackendAsyncResult::Pending(progress) = self.capture_core(Some(loop_id))? {
                return Ok(BackendAsyncResult::Pending(progress));
            }
        }
        let mut content = (*self
            .canonical_content
            .get(&loop_id)
            .ok_or_else(|| anyhow!("loop capture omitted target"))?)
        .as_ref()
        .clone();
        let mut audio = BTreeSet::new();
        for channel in &update.audio {
            if !audio.insert(channel.channel) {
                bail!("duplicate audio channel update");
            }
            let target = content
                .audio
                .get_mut(channel.channel)
                .ok_or_else(|| anyhow!("unknown audio channel"))?;
            target.samples.clone_from(&channel.samples);
            if let Some(offset) = channel.start_offset {
                target.start_offset = offset;
            }
            if let Some(alignment) = channel.capture_alignment_frames {
                target.capture_alignment_frames = alignment;
            }
            if let Some(preplay) = channel.preplay {
                target.preplay = preplay;
            }
        }
        let mut midi = BTreeSet::new();
        for channel in &update.midi {
            if !midi.insert(channel.channel) {
                bail!("duplicate MIDI channel update");
            }
            let target = content
                .midi
                .get_mut(channel.channel)
                .ok_or_else(|| anyhow!("unknown MIDI channel"))?;
            target.length = channel.length;
            target.start_state.clone_from(&channel.start_state);
            target.events.clone_from(&channel.events);
            if let Some(offset) = channel.start_offset {
                target.start_offset = offset;
            }
            if let Some(alignment) = channel.capture_alignment_frames {
                target.capture_alignment_frames = alignment;
            }
            if let Some(preplay) = channel.preplay {
                target.preplay = preplay;
            }
        }
        if let Some(length) = update.length {
            content.length = length;
        }
        let retained = self
            .canonical_imports
            .iter()
            .filter(|id| **id != loop_id)
            .try_fold(0u64, |size, id| {
                size.checked_add(
                    self.prepared_content
                        .get(id)
                        .filter(|p| p.matches(&self.canonical_content[id]))
                        .ok_or_else(|| anyhow!("retained content lacks a validation proof"))?
                        .bytes(),
                )
                .ok_or_else(|| anyhow!("retained media size overflow"))
            })?;
        let events = content
            .midi
            .iter()
            .map(|channel| channel.events.len())
            .sum::<usize>()
            + self
                .canonical_imports
                .iter()
                .filter(|id| **id != loop_id)
                .map(|id| {
                    self.canonical_content[id]
                        .midi
                        .iter()
                        .map(|channel| channel.events.len())
                        .sum::<usize>()
                })
                .sum::<usize>();
        if events > self.resource_limits.midi_events {
            bail!("retained canonical MIDI exceeds the host event budget");
        }
        let content = Arc::new(content);
        let id = self.next_persistence;
        self.next_persistence = id
            .checked_add(1)
            .ok_or_else(|| anyhow!("persistence identity exhausted"))?;
        let mut work = VecDeque::new();
        let mut uploads = Vec::new();
        let track = &self.mirror.tracks[&self.mirror.loops[&loop_id].track];
        let roles = !matches!(track.topology, BackendTrackTopology::Direct { .. });
        let recorded_state = crate::processor_controls::recorded_state(
            &track.topology,
            content.recorded_processor_state.as_deref(),
        )?;
        // Primary is wet; auxiliary is dry. Canonical channel indices remain dry first.
        let mut pair = [None; 3];
        let mut bytes = 0;
        if content.length > 0
            || content.audio.iter().any(|c| !c.samples.is_empty())
            || content.midi.iter().any(|c| c.length > 0)
        {
            for index in 0..if roles { 3 } else { 2 } {
                let token = self.next_persistence;
                self.next_persistence = token
                    .checked_add(1)
                    .ok_or_else(|| anyhow!("persistence identity exhausted"))?;
                let width = usize::from(self.limits().strip_audio_channels);
                let upload = if roles && index != 1 {
                    Upload::audio_role(
                        token,
                        Arc::clone(&content),
                        width,
                        if index == 0 {
                            BackendChannelMode::Wet
                        } else {
                            BackendChannelMode::Dry
                        },
                    )?
                } else {
                    Upload::new(token, index == 1, Arc::clone(&content), width)?
                };
                bytes += upload.description.bytes;
                if bytes > self.resource_limits.media_bytes {
                    bail!("prepared loop content exceeds the host budget");
                }
                uploads.push(token);
                pair[index] = Some(token);
                work.push_back(Work::Upload {
                    upload,
                    begun: false,
                });
                work.push_back(Work::Request(Request::ContentWindow {
                    id: token,
                    frames: u64::from(content.length),
                    windows: crate::prepared::windows(&content),
                }));
            }
        }
        work.push_back(Work::Request(Request::ContentApply {
            loop_id: self.mirror.loops[&loop_id].slot,
            frames: u64::from(content.length),
            dry_audio: pair[2],
            recorded_state,
            audio: pair[0],
            midi: pair[1],
        }));
        self.replacement = Some(crate::Replacement {
            mapping: Default::default(),
            candidate: None,
            job: Job::new(
                id,
                work,
                uploads,
                false,
                [Arc::clone(&content)],
                usize::from(self.limits().strip_audio_channels),
                self.resource_limits.media_bytes.saturating_sub(retained),
            )?,
            content: BTreeMap::from([(loop_id, content)]),
            connection_failures: Vec::new(),
            loop_target: Some(loop_id),
            published: false,
        });
        self.import_content(loop_id, update)
    }
}
