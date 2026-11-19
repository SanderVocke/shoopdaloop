use super::*;
use crate::midi::MidiJob;

impl MidiSession {
    pub(crate) fn execute_job(
        &mut self,
        job: MidiJob,
        base: u32,
        q: u32,
        events: &[InputEvent],
        sink: &mut impl FnMut(OutputEvent) -> Result<(), Error>,
    ) -> Result<(), Error> {
        match job {
            MidiJob::Begin { frame } => {
                self.next_frame = Some(frame);
                self.owners.fill(0);
                self.heap.clear();
                for c in &mut self.contexts {
                    c.blocked = false;
                }
                self.drain_recovery(base, sink)
            }
            MidiJob::FinishCapture { context } => {
                let i = usize::from(context);
                self.work.context_visits += 1;
                if self.contexts[i].mode == Mode::Recording {
                    if let Err(error) = self.finish_recording(i) {
                        self.contexts[i].status = TakeStatus::Incomplete;
                        return Err(error);
                    }
                }
                Ok(())
            }
            MidiJob::BeginCapture { context, frame } => {
                self.work.context_visits += 1;
                if let Err(error) = self.start_recording(usize::from(context), frame) {
                    self.contexts[usize::from(context)].status = TakeStatus::Incomplete;
                    return Err(error);
                }
                Ok(())
            }
            MidiJob::Observe => self.observe_window(base, q, events),
            MidiJob::Append { context } => {
                self.append_window(usize::from(context), base, q, events)
            }
            MidiJob::Cleanup { context, record } => {
                let i = usize::from(context);
                let b = self.contexts[i].binding.ok_or(Error::InvalidState)?;
                if self.recovery[usize::from(b.destination)] != 32 {
                    return Ok(());
                }
                if let Err(error) = self.prepare_cleanup(i, record, base, sink) {
                    self.contexts[i].blocked = true;
                    self.owners[usize::from(b.destination)] |= b.channels;
                    return Err(error);
                }
                Ok(())
            }
            MidiJob::Claim {
                context,
                playing,
                established,
            } => {
                let c = &mut self.contexts[usize::from(context)];
                if c.blocked || c.active.is_some() != established || !playing {
                    return Ok(());
                }
                let b = c.binding.ok_or(Error::InvalidState)?;
                let owned = &mut self.owners[usize::from(b.destination)];
                if *owned & b.channels != 0 {
                    c.blocked = true;
                    c.desynchronized = true;
                    return Err(Error::Unsupported);
                }
                *owned |= b.channels;
                Ok(())
            }
            MidiJob::ReadUntil { context, record } => {
                let i = usize::from(context);
                let b = self.contexts[i].binding.ok_or(Error::InvalidState)?;
                if self.contexts[i].blocked || self.recovery[usize::from(b.destination)] != 32 {
                    return Ok(());
                }
                if let Err(error) = self.prepare_playback(i, record, base, sink) {
                    if !matches!(error, Error::Busy | Error::Incomplete | Error::Unsupported)
                        && !self.contexts[i].cleaning
                    {
                        self.fail_destination(b.destination);
                    }
                    return Err(error);
                }
                if record.mode == Mode::Playing && record.position < record.content_length {
                    self.push_head(i, base, q)?;
                }
                Ok(())
            }
            MidiJob::Output => self.output_window(base, q, sink),
            MidiJob::Complete { context, mode } => {
                let c = &mut self.contexts[usize::from(context)];
                c.mode = mode;
                if c.active.is_some() && !c.cleaning && !c.blocked && !c.desynchronized {
                    c.position = c.position.saturating_add(u64::from(q));
                }
                Ok(())
            }
            MidiJob::Maintain => {
                self.store.maintenance(self.limits.maintenance_steps);
                self.transfer_events = 64;
                self.transfer_cells = 256;
                self.transfer_metadata = 64;
                self.publication_bindings = 128;
                Ok(())
            }
        }
    }
    fn observe_window(&mut self, base: u32, q: u32, events: &[InputEvent]) -> Result<(), Error> {
        let start = self
            .next_frame
            .ok_or(Error::InvalidState)?
            .checked_sub(u64::from(q))
            .ok_or(Error::InvalidState)?;
        let mut error = None;
        for seed in &mut self.seeds {
            if let Some(id) = seed.take() {
                self.store.release_snapshot(id)?;
            }
        }
        for event in events
            .iter()
            .filter(|e| e.offset >= base && e.offset < base + q)
        {
            let input = usize::from(event.input);
            let before = self.inputs[input].steps();
            if let Err(e) = self.histories[input].observe(
                &mut self.store,
                &mut self.maps,
                &self.outputs,
                &mut self.inputs[input],
                start + u64::from(event.offset - base),
                event.message,
            ) {
                error.get_or_insert(e);
            }
            self.work.input_steps = self
                .work
                .input_steps
                .saturating_add(self.inputs[input].steps() - before);
        }
        for h in &mut self.histories {
            if let Err(e) = h.advance(&mut self.store, start + u64::from(q)) {
                error.get_or_insert(e);
            }
        }
        error.map_or(Ok(()), Err)
    }
    fn append_window(
        &mut self,
        index: usize,
        base: u32,
        q: u32,
        events: &[InputEvent],
    ) -> Result<(), Error> {
        let c = &mut self.contexts[index];
        let b = c.binding.ok_or(Error::InvalidState)?;
        let Some(writer) = c.writer else {
            return Ok(());
        };
        if c.status != TakeStatus::Recording {
            return Ok(());
        }
        let mut error = None;
        for event in events
            .iter()
            .filter(|e| e.input == b.input && e.offset >= base && e.offset < base + q)
        {
            if event.message.as_bytes()[0] != 0xff
                && b.channels & (1 << (event.message.as_bytes()[0] & 15)) == 0
            {
                continue;
            }
            let frame = self.store.describe(writer)?.frames + u64::from(event.offset - base);
            if self.store.describe(writer)?.events >= c.record_limit {
                c.status = TakeStatus::Incomplete;
                self.store.mark_incomplete(writer)?;
                error.get_or_insert(Error::Capacity);
                break;
            }
            if let Err(e) = self.store.append(writer, frame, event.message) {
                c.status = TakeStatus::Incomplete;
                error.get_or_insert(e);
                break;
            }
        }
        if c.status == TakeStatus::Recording {
            let end = self
                .store
                .describe(writer)?
                .frames
                .checked_add(u64::from(q))
                .ok_or(Error::Overflow)?;
            if let Err(e) = self.store.advance(writer, end) {
                c.status = TakeStatus::Incomplete;
                error.get_or_insert(e);
            }
        }
        error.map_or(Ok(()), Err)
    }
    fn output_window(
        &mut self,
        base: u32,
        q: u32,
        sink: &mut impl FnMut(OutputEvent) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let mut error = None;
        while let Some(Reverse((offset, index))) = self.heap.pop() {
            let i = usize::from(index);
            let c = &self.contexts[i];
            let b = c.binding.unwrap();
            if self.recovery[usize::from(b.destination)] != 32 {
                continue;
            }
            let event = self
                .store
                .read(c.active.unwrap(), c.cursor)?
                .ok_or(Error::InvalidState)?;
            if event.message.as_bytes()[0] == 0xff && b.channels != u16::MAX {
                self.fail_destination(b.destination);
                error.get_or_insert(Error::Unsupported);
                continue;
            }
            match self.emit(
                OutputEvent {
                    destination: b.destination,
                    offset,
                    message: event.message,
                },
                sink,
            ) {
                Ok(()) => {
                    self.update_activity(i, event.message)?;
                    self.contexts[i].cursor += 1;
                    self.push_head(i, base, q)?;
                }
                Err(e) => {
                    self.fail_destination(b.destination);
                    error.get_or_insert(e);
                }
            }
        }
        error.map_or(Ok(()), Err)
    }
}
