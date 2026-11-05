use crate::{requests::Purpose, EngineClock, RtBackend};
use anyhow::{anyhow, Result};
use shoop_backend::BackendLoopId;
use shoop_rt_protocol::{Request, TimingEdit, TimingState};
use shoop_rt_transport::carrier::Carrier;

impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    pub(crate) fn send_timing(&mut self, id: BackendLoopId, edit: TimingEdit) -> Result<()> {
        self.pump();
        self.ready()?;
        if !self
            .capabilities
            .contains(shoop_rt_protocol::Capabilities::LOOP_TIMING)
        {
            return Err(anyhow!(
                shoop_app_api::BackendCapability::LoopTimingEdits.unavailable_reason()
            ));
        }
        if self.replacement.is_some() {
            return Err(anyhow!("a persistence operation is active"));
        }
        let entry = self
            .mirror
            .live_loop(id)
            .ok_or_else(|| anyhow!("unknown loop"))?;
        if !edit.canonical() {
            return Err(anyhow!("noncanonical timing edit"));
        }
        self.requests.push(
            Request::LoopTiming {
                loop_id: entry.slot,
                expected_revision: entry.timing.revision,
                expected_content: entry.rt_content_revision,
                edit,
            },
            Purpose::Timing(id),
        );
        Ok(())
    }
    pub(crate) fn apply_timing(&mut self, id: BackendLoopId, state: TimingState) {
        let Some(entry) = self.mirror.loops.get_mut(&id) else {
            return;
        };
        if state.revision < entry.timing.revision
            || state.content_revision < entry.rt_content_revision
        {
            return;
        }
        entry.timing = state;
        if entry.view.mode != shoop_rt_protocol::LoopMode::Recording {
            entry.view.length = state.length;
            if state.length > 0 {
                entry.view.position %= state.length;
            } else {
                entry.view.position = 0;
            }
        }
        if let Some(content) = self.canonical_content.get_mut(&id) {
            let image = alloc::sync::Arc::make_mut(content);
            image.length = state.length as u32;
            for (channel, window) in image.audio.iter_mut().zip(state.windows.audio) {
                channel.start_offset = window.start_offset;
                channel.capture_alignment_frames = window.capture_alignment;
                channel.preplay = window.preplay;
            }
            if let Some(channel) = image.midi.first_mut() {
                channel.start_offset = state.windows.midi.start_offset;
                channel.capture_alignment_frames = state.windows.midi.capture_alignment;
                channel.preplay = state.windows.midi.preplay;
            }
            if let Some(proof) = self
                .prepared_content
                .get(&id)
                .and_then(|proof| proof.rebind_windows(alloc::sync::Arc::clone(content)))
            {
                self.prepared_content.insert(id, proof);
            } else {
                self.prepared_content.remove(&id);
            }
        }
    }
}

impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    pub(crate) fn audio_timing(
        &self,
        id: BackendLoopId,
        mut data: shoop_backend::BackendAudioData,
    ) -> shoop_backend::BackendAudioData {
        if let Some(entry) = self.mirror.loops.get(&id) {
            for (channel, window) in data.channels.iter_mut().zip(entry.timing.windows.audio) {
                channel.start_offset = window.start_offset;
                channel.capture_alignment_frames = window.capture_alignment;
                channel.preplay = window.preplay;
                if let Some(frames) = window.raw_frames {
                    if (frames as usize) < channel.samples.len() {
                        channel.samples =
                            alloc::sync::Arc::new(channel.samples[..frames as usize].to_vec());
                    }
                }
            }
        }
        data
    }
    pub(crate) fn midi_timing(
        &self,
        id: BackendLoopId,
        mut data: shoop_backend::BackendMidiData,
    ) -> shoop_backend::BackendMidiData {
        if let Some(entry) = self.mirror.loops.get(&id) {
            for channel in &mut data.channels {
                let window = entry.timing.windows.midi;
                channel.start_offset = window.start_offset;
                channel.capture_alignment_frames = window.capture_alignment;
                channel.preplay = window.preplay;
                channel.length = window.raw_frames.unwrap_or(entry.timing.raw_midi_frames) as u32;
            }
        }
        data
    }
}
