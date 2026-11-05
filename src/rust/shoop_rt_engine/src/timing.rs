use crate::endpoint::{
    completion_refusal, failure_refusal, loop_id, ControlEndpoint, Purpose, RuntimeResult,
};
use core::sync::atomic::Ordering;
use rt_runtime::Revision;
use shoop_loops::{
    runtime::{Cut, Operation, Reply},
    windows,
};
use shoop_rt_protocol::{
    ChannelWindow, LoopSlotId, Outcome, PlaybackWindows, Refusal, TimingEdit, TimingState,
};

fn window(value: windows::ChannelWindow) -> ChannelWindow {
    ChannelWindow {
        gain_bits: value.gain_bits,
        start_offset: value.start_offset,
        capture_alignment: value.capture_alignment,
        preplay: value.preplay,
        raw_frames: value.raw_frames,
    }
}
pub(crate) fn state(cut: Cut) -> TimingState {
    TimingState {
        revision: cut.timing_revision,
        content_revision: cut.state.content_revision.0,
        length: cut.state.content_length,
        raw_audio_frames: cut.raw_audio_frames,
        raw_midi_frames: cut.raw_midi_frames,
        windows: PlaybackWindows {
            audio: cut.windows.audio.map(window),
            midi: window(cut.windows.midi),
        },
    }
}
impl ControlEndpoint {
    pub(crate) fn timing_pending(&self) -> bool {
        self.pending
            .iter()
            .flatten()
            .any(|pending| matches!(pending.purpose, Purpose::Timing { .. }))
    }
    pub(crate) fn current_timing(&self, entry: crate::model::Loop) -> TimingState {
        let mut timing = entry.timing;
        let cells = &self.cells.loops[entry.id.0.index as usize];
        if cells.generation.load(Ordering::Acquire) != entry.id.0.generation {
            return timing;
        }
        let revision = cells.content_revision.load(Ordering::Acquire);
        if revision != timing.content_revision || timing == TimingState::default() {
            let frames = cells.length.load(Ordering::Acquire);
            let shape = self
                .model
                .strip(entry.strip)
                .expect("live loop strip")
                .shape;
            timing.content_revision = revision;
            for window in timing
                .windows
                .audio
                .iter_mut()
                .chain(core::iter::once(&mut timing.windows.midi))
            {
                window.raw_frames = None;
            }
            if frames == 0 && cells.mode.load(Ordering::Acquire) == 0 {
                timing.windows = PlaybackWindows::default();
            }
            timing.length = frames;
            timing.raw_audio_frames = if shape.audio_outputs > 0 { frames } else { 0 };
            timing.raw_midi_frames = if shape.midi_output { frames } else { 0 };
        }
        timing
    }
    pub(crate) fn timing_request(
        &mut self,
        correlation: u64,
        target: LoopSlotId,
        expected_revision: u64,
        expected_content: u64,
        edit: TimingEdit,
    ) -> Result<Option<Outcome>, Refusal> {
        if self.family.id.is_some() || self.replacement.id.is_some() {
            return Err(Refusal::Busy);
        }
        let entry = self.model.find_loop(target).ok_or(Refusal::InvalidId)?;
        let shape = self
            .model
            .strip(entry.strip)
            .ok_or(Refusal::InvalidId)?
            .shape;
        if !edit.canonical()
            || edit.length.is_some_and(|length| {
                length > u32::MAX as u64 || length % u64::from(self.limits.quantum) != 0
            })
        {
            return Err(Refusal::InvalidValue);
        }
        for update in &edit.channels[..usize::from(edit.count)] {
            if if update.midi {
                !shape.midi_output || update.channel != 0
            } else {
                update.channel >= shape.audio_outputs
            } {
                return Err(Refusal::InvalidId);
            }
        }
        let edit = windows::Edit {
            length: edit.length,
            start_offset: edit.start_offset,
            preplay: edit.preplay,
            count: edit.count,
            channels: edit.channels.map(|update| windows::ChannelUpdate {
                channel: update.channel,
                midi: update.midi,
                start_offset: update.start_offset,
                preplay: update.preplay,
            }),
        };
        self.submit_loop(
            self.at(),
            Operation::EditWindows {
                target: loop_id(target),
                expected_revision,
                expected_content: Revision(expected_content),
                edit,
            },
            Purpose::Timing {
                slot: target.0.index as usize,
                correlation: Some(correlation),
            },
        )?;
        Ok(None)
    }
    pub(crate) fn timing_result(
        &mut self,
        slot: usize,
        correlation: Option<u64>,
        result: RuntimeResult,
    ) {
        let outcome = match result {
            RuntimeResult::Completion(outcome, command, retired) => {
                self.reclaim(command);
                self.retire(retired);
                completion_refusal(outcome).map(Outcome::Refused)
            }
            RuntimeResult::Execution(execution) => Some(match execution.operation_result {
                Some(Reply::Timing(cut)) => {
                    let timing = state(cut);
                    for model in [&mut self.model, &mut self.next] {
                        if let Some(entry) = model
                            .loops
                            .get_mut(slot)
                            .and_then(Option::as_mut)
                            .filter(|entry| loop_id(entry.id) == cut.state.id)
                        {
                            entry.timing = timing;
                        }
                    }
                    self.dirty[slot] = true;
                    Outcome::TimingApplied(timing)
                }
                Some(Reply::Failed(failure)) => Outcome::Refused(failure_refusal(failure)),
                _ => Outcome::Refused(Refusal::Internal),
            }),
        };
        if let (Some(correlation), Some(outcome)) = (correlation, outcome) {
            self.answer(correlation, outcome);
        }
    }
}

pub(crate) fn loop_windows(windows: PlaybackWindows) -> windows::Windows {
    let channel = |value: ChannelWindow| windows::ChannelWindow {
        gain_bits: value.gain_bits,
        start_offset: value.start_offset,
        capture_alignment: value.capture_alignment,
        preplay: value.preplay,
        raw_frames: value.raw_frames,
    };
    windows::Windows {
        audio: windows.audio.map(channel),
        midi: channel(windows.midi),
    }
}

impl ControlEndpoint {
    pub(crate) fn publish_content_timing(&mut self, target: LoopSlotId, mut timing: TimingState) {
        for model in [&mut self.model, &mut self.next] {
            if let Some(entry) = model
                .loops
                .get_mut(target.0.index as usize)
                .and_then(Option::as_mut)
                .filter(|entry| entry.id == target)
            {
                timing.revision = entry.timing.revision;
                entry.timing = timing;
            }
        }
        self.dirty[target.0.index as usize] = true;
    }
}
