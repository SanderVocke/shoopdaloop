use alloc::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
    vec::Vec,
};
use anyhow::{anyhow, bail, Result};
use shoop_backend::*;
use shoop_rt_protocol::{LoopSlotId, ReplacementEdit, Request};
use shoop_rt_transport::carrier::Carrier;

use crate::{
    mirror::LoopView,
    persistence::{Job, Work},
    prepared::Upload,
    EngineClock, RtBackend,
};

impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    pub(crate) fn duplicate_content(
        &mut self,
        source: BackendLoopId,
        target: Option<BackendLoopId>,
        additions: &[BackendTrackId],
        row_sync: Option<BackendLoopId>,
    ) -> Result<BackendAsyncResult<BackendLoopDuplication>> {
        if self.duplication_result.is_some() {
            let progress = match self.persistence_progress() {
                Ok(progress) => progress,
                Err(error) => {
                    self.duplication_result = None;
                    return Err(error);
                }
            };
            return match progress {
                BackendAsyncResult::Pending(progress) => Ok(BackendAsyncResult::Pending(progress)),
                BackendAsyncResult::Ready(_) => Ok(BackendAsyncResult::Ready(
                    self.duplication_result.take().unwrap(),
                )),
            };
        }
        self.pump();
        self.ready()?;
        if self.replacement.is_some()
            || self.input.is_some()
            || !self.composites.jobs.is_empty()
            || !self.composites.controls.is_empty()
        {
            bail!("settle conflicting media and composite operations before duplication");
        }
        if !self.requests.is_empty() {
            return Ok(BackendAsyncResult::Pending(BackendOperationProgress {
                key: 0,
                kind: BackendOperationKind::LoopContentReplacement,
                completed: 0,
                total: None,
            }));
        }
        let from = self
            .mirror
            .live_loop(source)
            .ok_or_else(|| anyhow!("unknown duplication source"))?
            .clone();
        let track = from.track.ok_or_else(|| anyhow!("source has no track"))?;
        if from.view.recording_armed
            || from.view.mode.records()
            || from.view.next_mode.is_some_and(|mode| mode.records())
        {
            bail!("recording sources cannot be duplicated");
        }
        if target == Some(source) {
            bail!("a loop cannot be duplicated onto itself");
        }
        if let Some(id) = target {
            let entry = self
                .mirror
                .live_loop(id)
                .ok_or_else(|| anyhow!("unknown duplication destination"))?;
            if entry.track != Some(track)
                || entry.view.recording_armed
                || entry.view.mode.records()
                || entry.view.next_mode.is_some_and(|mode| mode.records())
            {
                bail!("destination must be on the same track and not recording");
            }
        }
        let captured = if from.program.is_none() {
            match self.capture_core(Some(source))? {
                BackendAsyncResult::Pending(progress) => {
                    return Ok(BackendAsyncResult::Pending(progress))
                }
                BackendAsyncResult::Ready(_) => Some(
                    self.canonical_content
                        .get(&source)
                        .ok_or_else(|| anyhow!("capture omitted duplication source"))?
                        .clone(),
                ),
            }
        } else {
            None
        };
        if row_sync.is_some_and(|id| self.mirror.live_loop(id).is_none()) {
            bail!("unknown row synchronization source");
        }
        let mut candidate = self.mirror.clone();
        let mut added = Vec::new();
        if target.is_some() && !additions.is_empty() {
            bail!("an explicit destination cannot add rows");
        }
        for &owner in additions {
            candidate
                .live_track(owner)
                .ok_or_else(|| anyhow!("unknown row track"))?;
            let slot = LoopSlotId(
                candidate
                    .loop_slots
                    .reserve()
                    .ok_or_else(|| anyhow!("aligned row exceeds prepared loop capacity"))?,
            );
            let id = candidate.next_loop_id();
            let mut empty = from.clone();
            empty.slot = slot;
            empty.track = Some(owner);
            empty.program = None;
            empty.program_spans.clear();
            empty.composite = None;
            empty.recorded = None;
            empty.repeat_sync = row_sync.is_some();
            empty.sync = row_sync;
            empty.gain = 1.;
            empty.balance = 0.;
            empty.view = LoopView::default();
            empty.timing = Default::default();
            empty.content = 0;
            empty.rt_content_revision = 0;
            empty.meter = Default::default();
            candidate.loops.insert(id, empty);
            candidate.slot_loops.insert(slot, id);
            candidate.tracks.get_mut(&owner).unwrap().loops.push(id);
            added.push((owner, id));
        }
        let target = target
            .or_else(|| {
                added
                    .iter()
                    .find(|(owner, _)| *owner == track)
                    .map(|(_, id)| *id)
            })
            .ok_or_else(|| anyhow!("duplication has no destination"))?;
        let old_slot = self.mirror.loops.get(&target).map(|entry| entry.slot);
        let slot = if let Some(old) = old_slot {
            candidate.slot_loops.remove(&old);
            LoopSlotId(
                candidate
                    .loop_slots
                    .reserve()
                    .ok_or_else(|| anyhow!("loop identity exhausted"))?,
            )
        } else {
            candidate.loops[&target].slot
        };
        if let Some(old) = old_slot {
            candidate.loop_slots.release(old.0);
        }
        let mut entry = from.clone();
        entry.slot = slot;
        entry.view = LoopView {
            length: from.view.length,
            ..LoopView::default()
        };
        entry.composite = None;
        entry.content = candidate.loops[&target]
            .content
            .checked_add(1)
            .ok_or_else(|| anyhow!("content revision exhausted"))?;
        entry.rt_content_revision = 0;
        entry.timing.content_revision = 0;
        entry.timing.revision = 0;
        entry.confirmed = true;
        entry.meter = Default::default();
        if let Some(program) = &mut entry.program {
            program.loop_id = slot;
            program.id = 0;
            program.previous = None;
            program.expected_version = 0;
        }
        candidate.loops.insert(target, entry);
        candidate.slot_loops.insert(slot, target);
        if let Some(old) = old_slot {
            for other in candidate.loops.values_mut() {
                if let Some(program) = &mut other.program {
                    if program.sync == Some(old) {
                        program.sync = Some(slot);
                    }
                }
                for span in &mut other.program_spans {
                    if span.target == old {
                        span.target = slot;
                    }
                }
                other
                    .program_spans
                    .sort_by_key(|span| (span.target, span.start));
            }
        }
        self.mirror.reserve_image_identities(&candidate);
        let token = self.next_persistence;
        self.next_persistence = token
            .checked_add(1)
            .ok_or_else(|| anyhow!("operation identity exhausted"))?;
        let mut work = VecDeque::from([Work::Request(Request::PatchBegin { id: token })]);
        if let Some(previous) = old_slot {
            work.push_back(Work::Request(Request::ReplacementEdit {
                id: token,
                edit: ReplacementEdit::Retarget {
                    previous,
                    loop_id: slot,
                },
            }));
        }
        let mut content = BTreeMap::new();
        let mut uploads = Vec::new();
        let topology = &candidate.tracks[&track].topology;
        let roles = !matches!(topology, BackendTrackTopology::Direct { .. });
        let mut pair = [None; 3];
        if let Some(captured) = captured {
            let mut image = (*captured).clone();
            image.source_id = target.raw();
            image.gain = from.gain;
            image.balance = from.balance;
            let image = Arc::new(image);
            if image.length > 0
                || image.audio.iter().any(|c| !c.samples.is_empty())
                || image.midi.iter().any(|c| c.length > 0)
            {
                for index in 0..if roles { 3 } else { 2 } {
                    let id = self.next_persistence;
                    self.next_persistence = id
                        .checked_add(1)
                        .ok_or_else(|| anyhow!("content identity exhausted"))?;
                    let width = usize::from(self.limits().strip_audio_channels);
                    let upload = if roles && index != 1 {
                        Upload::audio_role(
                            id,
                            image.clone(),
                            width,
                            if index == 0 {
                                BackendChannelMode::Wet
                            } else {
                                BackendChannelMode::Dry
                            },
                        )?
                    } else {
                        Upload::new(id, index == 1, image.clone(), width)?
                    };
                    uploads.push(id);
                    pair[index] = Some(id);
                    work.push_back(Work::Upload {
                        upload,
                        begun: false,
                    });
                    work.push_back(Work::Request(Request::ContentWindow {
                        id,
                        frames: u64::from(image.length),
                        windows: crate::prepared::windows(&image),
                    }));
                }
            }
            content.insert(target, image);
        }
        for &(owner, id) in &added {
            if id == target {
                continue;
            }
            work.push_back(Work::Request(Request::ReplacementEdit {
                id: token,
                edit: ReplacementEdit::Loop {
                    loop_id: candidate.loops[&id].slot,
                    strip: candidate.tracks[&owner].strip,
                    sync: row_sync.map(|id| candidate.loops[&id].slot),
                    frames: 0,
                    dry_audio: None,
                    recorded_state: None,
                    audio: None,
                    midi: None,
                    gain: 1.,
                    balance: 0.,
                },
            }));
        }
        if row_sync.is_some() {
            for &(_, id) in &added {
                if id != target {
                    work.push_back(Work::Request(Request::ReplacementEdit {
                        id: token,
                        edit: ReplacementEdit::RepeatSync {
                            loop_id: candidate.loops[&id].slot,
                            enabled: true,
                        },
                    }));
                }
            }
        }
        let entry = &candidate.loops[&target];
        if let Some(program) = entry.program {
            work.push_back(Work::Request(Request::ReplacementEdit {
                id: token,
                edit: ReplacementEdit::Composite(program),
            }));
            for (index, span) in entry.program_spans.iter().copied().enumerate() {
                work.push_back(Work::Request(Request::ReplacementEdit {
                    id: token,
                    edit: ReplacementEdit::CompositeSpan {
                        loop_id: slot,
                        index: index as u32,
                        span,
                    },
                }));
            }
        } else {
            let image = &content[&target];
            work.push_back(Work::Request(Request::ReplacementEdit {
                id: token,
                edit: ReplacementEdit::Loop {
                    loop_id: slot,
                    strip: candidate.tracks[&track].strip,
                    sync: entry.sync.map(|id| candidate.loops[&id].slot),
                    frames: u64::from(image.length),
                    dry_audio: pair[2],
                    recorded_state: crate::processor_controls::recorded_state(
                        topology,
                        image.recorded_processor_state.as_deref(),
                    )?,
                    audio: pair[0],
                    midi: pair[1],
                    gain: image.gain,
                    balance: image.balance,
                },
            }));
        }
        work.push_back(Work::Request(Request::ReplacementEdit {
            id: token,
            edit: ReplacementEdit::RepeatSync {
                loop_id: slot,
                enabled: entry.repeat_sync,
            },
        }));
        work.push_back(Work::Request(Request::ReplacementPrepare { id: token }));
        work.push_back(Work::Request(Request::ReplacementCommit { id: token }));
        if let Some(loop_id) = old_slot {
            work.push_back(Work::Request(Request::RemoveLoop { loop_id }));
        }
        let retained = self
            .canonical_imports
            .iter()
            .filter(|id| **id != target)
            .try_fold(0u64, |total, id| {
                total
                    .checked_add(self.prepared_content[id].bytes())
                    .ok_or_else(|| anyhow!("retained media size overflow"))
            })?;
        let events = content
            .values()
            .chain(
                self.canonical_imports
                    .iter()
                    .filter(|id| **id != target)
                    .map(|id| &self.canonical_content[id]),
            )
            .try_fold(0usize, |count, image| {
                image.midi.iter().try_fold(count, |count, channel| {
                    count
                        .checked_add(channel.events.len())
                        .ok_or_else(|| anyhow!("MIDI event count overflow"))
                })
            })?;
        if events > self.resource_limits.midi_events {
            bail!("duplication exceeds the retained MIDI event budget");
        }
        let job = Job::new(
            token,
            work,
            uploads,
            true,
            content.values().cloned(),
            usize::from(self.limits().strip_audio_channels),
            self.resource_limits.media_bytes.saturating_sub(retained),
        )?;
        self.replacement = Some(crate::Replacement {
            patch: true,
            mapping: Default::default(),
            candidate: Some(candidate),
            job,
            content,
            connection_failures: Vec::new(),
            loop_target: None,
            published: false,
        });
        self.duplication_result = Some(BackendLoopDuplication { target, added });
        self.duplicate_content(source, Some(target), &[], row_sync)
    }
}
