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
            let content = alloc::sync::Arc::make_mut(content);
            content.length = state.length as u32;
            for (channel, window) in content.audio.iter_mut().zip(state.windows.audio) {
                channel.start_offset = window.start_offset;
                channel.capture_alignment_frames = window.capture_alignment;
                channel.preplay = window.preplay;
            }
            if let Some(channel) = content.midi.first_mut() {
                channel.start_offset = state.windows.midi.start_offset;
                channel.capture_alignment_frames = state.windows.midi.capture_alignment;
                channel.preplay = state.windows.midi.preplay;
            }
            self.prepared_content.remove(&id);
        }
    }
}
