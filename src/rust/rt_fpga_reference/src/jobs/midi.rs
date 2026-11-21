use super::{Job, JobProcessor};
use crate::midi::{self, BufferLimits, Error, Event, OutputEvent, TakeStatus, Work};

impl JobProcessor {
    pub fn prepare_midi(&mut self, limits: BufferLimits) -> Result<(), Error> {
        self.midi = Some(midi::Buffers::prepare(limits)?);
        Ok(())
    }
    pub fn midi_work(&self) -> Work {
        self.midi.as_ref().map_or(Work::default(), |b| b.work)
    }
    pub(crate) fn begin_midi_callback(
        &mut self,
        events: &[midi::InputEvent],
        frames: usize,
    ) -> Result<(), Error> {
        let buffers = self.midi.as_mut().ok_or(Error::InvalidState)?;
        if frames > u32::MAX as usize
            || events.windows(2).any(|w| w[0].offset > w[1].offset)
            || events
                .iter()
                .any(|e| e.offset as usize >= frames || e.input >= buffers.limits.input_ports)
        {
            return Err(Error::InvalidEvent);
        }
        buffers.work = Work::default();
        Ok(())
    }
    pub(super) fn execute_midi(
        &mut self,
        job: Job,
        io: &mut midi::Io<'_>,
        base: usize,
    ) -> Result<(), Error> {
        let buffers = self.midi.as_mut().ok_or(Error::InvalidState)?;
        match job {
            Job::MidiEmpty { destination } => {
                buffers.destination(destination)?;
                buffers.publish(destination, buffers.events.len());
            }
            Job::MidiInput { port, destination } => {
                buffers.destination(destination)?;
                if port >= buffers.limits.input_ports {
                    return Err(Error::InvalidState);
                }
                let start = buffers.events.len();
                let first = io.inputs.partition_point(|e| (e.offset as usize) < base);
                for (ordinal, event) in io
                    .inputs
                    .iter()
                    .enumerate()
                    .skip(first)
                    .take_while(|(_, e)| (e.offset as usize) < base + self.quantum)
                {
                    if event.input == port {
                        if !buffers.observed_inputs[port as usize] {
                            if let Some(state) = &mut io.memory.expression {
                                state.observe_input(port, event.message)?;
                            }
                        }
                        buffers.push(Event {
                            offset: event.offset - base as u32,
                            source: 0x10000 | u32::from(port),
                            order: u32::try_from(ordinal).map_err(|_| Error::Overflow)?,
                            message: event.message,
                        })?;
                    }
                }
                buffers.observed_inputs[port as usize] = true;
                buffers.publish(destination, start);
            }
            Job::MidiReadView {
                owner,
                sequence,
                cursor,
                position,
                frames,
                restart,
                channels,
                destination,
            } => {
                buffers.destination(destination)?;
                let view = io.memory.view(sequence)?.ok_or(Error::InvalidState)?;
                let at = usize::from(cursor.0);
                let mut reader = *buffers.cursors.get(at).ok_or(Error::InvalidState)?;
                if restart || reader.owner != Some(owner) || reader.view != Some(view) {
                    if io.memory.store.describe(view)?.status != TakeStatus::Complete {
                        return Err(Error::Incomplete);
                    }
                    reader = midi::buffers::Cursor {
                        owner: Some(owner),
                        sequence: Some(sequence),
                        view: Some(view),
                        index: if position == 0 {
                            0
                        } else {
                            io.memory.store.lower_bound(view, position)?
                        },
                        next: None,
                        ended: false,
                    };
                }
                let start = buffers.events.len();
                let end = position
                    .checked_add(u64::from(frames))
                    .ok_or(Error::Overflow)?;
                loop {
                    if reader.next.is_none() && !reader.ended {
                        reader.next = io.memory.store.read(view, reader.index)?;
                        reader.ended = reader.next.is_none();
                        buffers.work.record_reads += u64::from(!reader.ended);
                    }
                    let Some(record) = reader.next else {
                        break;
                    };
                    if record.frame >= end {
                        break;
                    }
                    if record.frame >= position
                        && record
                            .message
                            .channel()
                            .is_none_or(|c| channels & (1 << c) != 0)
                    {
                        buffers.push(Event {
                            offset: (record.frame - position) as u32,
                            source: u32::from(cursor.0),
                            order: record.order,
                            message: record.message,
                        })?;
                    }
                    reader.index = reader.index.checked_add(1).ok_or(Error::Overflow)?;
                    reader.next = None;
                }
                buffers.cursors[at] = reader;
                buffers.publish(destination, start);
            }
            Job::MidiRestoreState {
                owner,
                sequence,
                position,
            } => {
                let view = io.memory.view(sequence)?.ok_or(Error::InvalidState)?;
                let state = io.memory.expression.as_mut().ok_or(Error::InvalidState)?;
                state.validate_owner(sequence.0, owner)?;
                state.restore(
                    sequence.0,
                    view,
                    position,
                    base as u32,
                    &io.memory.store,
                    &mut io.sink,
                )?;
            }
            Job::MidiCleanupState { owner, sequence } => {
                let state = io.memory.expression.as_mut().ok_or(Error::InvalidState)?;
                state.validate_owner(sequence.0, owner)?;
                state.cleanup(sequence.0, base as u32, &io.memory.store, &mut io.sink)?;
            }
            Job::MidiBeginCapture { sequence } => io.memory.begin_capture(sequence)?,
            Job::MidiFinishCapture { sequence } => io.memory.finish_capture(sequence)?,
            Job::MidiAppend {
                sequence,
                source,
                position,
                frames,
            } => {
                let span = buffers.source(source)?;
                let view = io.memory.view(sequence)?.ok_or(Error::InvalidState)?;
                for index in span.start..span.start + span.count {
                    let event = buffers.events[index];
                    if event.offset >= frames {
                        return Err(Error::InvalidEvent);
                    }
                    io.memory.store.append(
                        view,
                        position
                            .checked_add(u64::from(event.offset))
                            .ok_or(Error::Overflow)?,
                        event.message,
                    )?;
                    buffers.work.captured_events += 1;
                }
                io.memory.store.advance(
                    view,
                    position
                        .checked_add(u64::from(frames))
                        .ok_or(Error::Overflow)?,
                )?;
            }
            Job::MidiMerge {
                left,
                right,
                destination,
            } => {
                buffers.destination(destination)?;
                let a = buffers.source(left)?;
                let b = buffers.source(right)?;
                if a.count == 0 || b.count == 0 {
                    buffers.spans[usize::from(destination.0)] = if a.count == 0 { b } else { a };
                    return Ok(());
                }
                let count = a.count.checked_add(b.count).ok_or(Error::Overflow)?;
                if count > buffers.limits.scratch_events - buffers.events.len() {
                    return Err(Error::Capacity);
                }
                let start = buffers.events.len();
                let (mut ai, mut bi) = (0, 0);
                while ai < a.count || bi < b.count {
                    let from_a = bi == b.count
                        || ai < a.count
                            && buffers.events[a.start + ai].key()
                                <= buffers.events[b.start + bi].key();
                    let event = if from_a {
                        let e = buffers.events[a.start + ai];
                        ai += 1;
                        e
                    } else {
                        let e = buffers.events[b.start + bi];
                        bi += 1;
                        e
                    };
                    buffers.push(event)?;
                    buffers.work.merged_events += 1;
                }
                buffers.publish(destination, start);
            }
            Job::MidiOutput { source, port } => {
                if port >= buffers.limits.output_ports {
                    return Err(Error::InvalidState);
                }
                let span = buffers.source(source)?;
                for index in span.start..span.start + span.count {
                    let event = buffers.events[index];
                    (io.sink)(OutputEvent {
                        destination: port,
                        offset: (base as u32)
                            .checked_add(event.offset)
                            .ok_or(Error::Overflow)?,
                        message: event.message,
                    })?;
                    if let Some(state) = &mut io.memory.expression {
                        let owner = if event.source < 0x10000 {
                            buffers
                                .cursors
                                .get(event.source as usize)
                                .and_then(|c| c.sequence)
                                .map(|s| s.0)
                        } else {
                            None
                        };
                        state.accept(port, event.message, owner, &io.memory.store)?;
                    }
                    buffers.work.output_events += 1;
                }
            }
            _ => return Err(Error::InvalidState),
        }
        Ok(())
    }
}
