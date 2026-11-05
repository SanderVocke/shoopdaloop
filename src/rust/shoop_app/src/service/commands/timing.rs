use super::*;

impl Service {
    fn require_timing(&self) -> Result<(), CommandError> {
        if !self
            .backend
            .capabilities()
            .supports(BackendCapability::LoopTimingEdits)
        {
            return Err(CommandError::unsupported(
                BackendCapability::LoopTimingEdits,
            ));
        }
        Ok(())
    }
    pub(super) fn set_loop_timeline(
        &mut self,
        loop_id: LoopId,
        start: Option<i64>,
        preplay: Option<u64>,
        length: Option<u64>,
    ) -> CommandResult {
        self.require_timing()?;
        let entry = self
            .loops
            .get(&loop_id)
            .ok_or_else(|| CommandError::invalid("unknown loop"))?;
        let backend = entry.backend;
        let state = self
            .observed
            .loops
            .get(&backend)
            .ok_or_else(|| CommandError::invalid("loop state is unavailable"))?;
        let alignment = state
            .channel_timings
            .iter()
            .find(|channel| !channel.is_midi)
            .map_or(state.capture_alignment_frames, |channel| {
                channel.capture_alignment_frames
            });
        let start = start
            .map(|value| {
                value
                    .checked_sub(i64::from(alignment))
                    .and_then(|value| i32::try_from(value).ok())
                    .ok_or_else(|| {
                        CommandError::invalid("loop start is outside the supported frame range")
                    })
            })
            .transpose()?;
        let preplay = preplay
            .map(|value| {
                u32::try_from(value).map_err(|_| {
                    CommandError::invalid("pre-play is outside the supported frame range")
                })
            })
            .transpose()?;
        let length = length
            .map(|value| {
                u32::try_from(value).map_err(|_| {
                    CommandError::invalid("loop length is outside the supported frame range")
                })
            })
            .transpose()?;
        self.backend
            .set_loop_timing(backend, start, preplay, length)?;
        Ok(Effect::None)
    }
    pub(super) fn set_loop_channel_timings(
        &mut self,
        loop_id: LoopId,
        updates: &[ChannelTimelineUpdate],
    ) -> CommandResult {
        self.require_timing()?;
        let entry = self
            .loops
            .get(&loop_id)
            .ok_or_else(|| CommandError::invalid("unknown loop"))?;
        let state = self
            .observed
            .loops
            .get(&entry.backend)
            .ok_or_else(|| CommandError::invalid("loop state is unavailable"))?;
        if updates.len() > 9 {
            return Err(CommandError::invalid("too many channel updates"));
        }
        let mut seen = BTreeSet::new();
        let mut timings = Vec::with_capacity(updates.len());
        for update in updates {
            if !seen.insert(update.channel_id) {
                return Err(CommandError::invalid("duplicate channel update"));
            }
            let channel = state
                .channel_timings
                .iter()
                .find(|channel| {
                    let index = if channel.is_midi {
                        channel.channel + 129
                    } else {
                        channel.channel + 1
                    };
                    ChannelId::from_raw((loop_id.raw() << 8) | index as u64) == update.channel_id
                })
                .ok_or_else(|| CommandError::invalid("stale or foreign loop channel"))?;
            let start_offset = update
                .start_offset
                .map(|value| {
                    value
                        .checked_sub(i64::from(channel.capture_alignment_frames))
                        .and_then(|value| i32::try_from(value).ok())
                        .ok_or_else(|| {
                            CommandError::invalid(
                                "channel start is outside the supported frame range",
                            )
                        })
                })
                .transpose()?;
            let preplay = update
                .preplay_samples
                .map(|value| {
                    u32::try_from(value).map_err(|_| {
                        CommandError::invalid("pre-play is outside the supported frame range")
                    })
                })
                .transpose()?;
            timings.push(BackendChannelTimingUpdate {
                channel: channel.channel,
                is_midi: channel.is_midi,
                start_offset,
                preplay,
            });
        }
        self.backend
            .set_loop_channel_timings(entry.backend, &timings)?;
        Ok(Effect::None)
    }
}
