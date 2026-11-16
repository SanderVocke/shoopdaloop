use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime) struct GrabWorkId {
    scope: u32,
    slot: usize,
    generation: u32,
}
#[derive(Clone, Copy)]
struct Work {
    take: SharedTake,
    midi: Option<indexed_midi::GrabId>,
    ready: bool,
    failure: Option<Error>,
}
pub(super) struct GrabSlot {
    generation: u32,
    work: Option<Work>,
}
impl Default for GrabSlot {
    fn default() -> Self {
        Self {
            generation: 1,
            work: None,
        }
    }
}
impl<S: Sample> Shared<S> {
    /// The immediate input transaction reserves the complete finite replay grant
    /// before submitting any musical action to the frame resolver.
    pub(in crate::runtime) fn grab_window_immediate(
        &mut self,
        source: HistoryId,
        window: &crate::grab::Window,
        q: usize,
        io: &mut ResourceIo<'_, S>,
    ) -> Result<SharedTake, Error> {
        let id = self.begin_grab_work(source, window, q, io)?;
        let mut work = self.grab_work(id)?;
        let result = (|| {
            if let Some(midi) = work.midi {
                let grant = io.indexed_midi_grab_replay_bound(midi)?;
                for _ in 0..2 {
                    if io.progress_indexed_midi_grab(midi, grant)? == indexed_midi::Status::Ready {
                        work.take.midi = Some(io.finish_indexed_midi_grab(midi)?);
                        work.midi = None;
                        work.ready = true;
                        self.grab_building -= 1;
                        break;
                    }
                }
                if !work.ready {
                    return Err(Error::Busy);
                }
            }
            Ok(work.take)
        })();
        self.grabs[id.slot].work = Some(work);
        match result {
            Ok(take) => {
                self.track(take);
                self.release_grab_slot(id);
                Ok(take)
            }
            Err(error) => {
                self.cancel_grab_work(id, io)?;
                Err(error)
            }
        }
    }
    pub(in crate::runtime) fn begin_grab_work(
        &mut self,
        source: HistoryId,
        window: &crate::grab::Window,
        q: usize,
        io: &mut ResourceIo<'_, S>,
    ) -> Result<GrabWorkId, Error> {
        let slot = self
            .grabs
            .iter()
            .position(|slot| slot.work.is_none() && slot.generation != 0)
            .ok_or(Error::Capacity)?;
        let work = match self.grab_window(source, window, q, io) {
            Ok(take) => Work {
                take,
                midi: None,
                ready: true,
                failure: None,
            },
            Err(Error::Busy) => {
                if self.allowance == 0 {
                    return Err(Error::Busy);
                }
                let history = self.source(source.source)?;
                let midi_source = history.indexed_midi.ok_or(Error::Busy)?;
                let audio = history
                    .audio_source
                    .map(|id| io.grab_audio_window(id, window.source.clone(), window.frames))
                    .transpose()?;
                let take = SharedTake {
                    audio,
                    midi: None,
                    frames: window.frames,
                    audio_store: history.audio,
                    midi_store: history.midi,
                    indexed_midi_store: Some(midi_source.store()),
                };
                let midi = match io.prepare_indexed_midi_grab(
                    midi_source,
                    window.source.clone(),
                    window.frames,
                ) {
                    Ok(id) => id,
                    Err(error) => {
                        if let Some(audio) = audio {
                            io.remove(audio)?;
                        }
                        return Err(error);
                    }
                };
                self.track(take);
                self.grab_building += 1;
                Work {
                    take,
                    midi: Some(midi),
                    ready: false,
                    failure: None,
                }
            }
            Err(error) => return Err(error),
        };
        self.grabs[slot].work = Some(work);
        Ok(GrabWorkId {
            scope: self.scope,
            slot,
            generation: self.grabs[slot].generation,
        })
    }
    fn grab_work(&self, id: GrabWorkId) -> Result<Work, Error> {
        if id.scope != self.scope {
            return Err(Error::InvalidId);
        }
        self.grabs
            .get(id.slot)
            .filter(|slot| slot.generation == id.generation)
            .and_then(|slot| slot.work)
            .ok_or(Error::InvalidId)
    }
    pub(super) fn progress_grabs(&mut self, io: &mut ResourceIo<'_, S>) {
        let budget = if self.building > 0 {
            self.remaining.div_ceil(2)
        } else {
            self.remaining
        };
        for _ in 0..budget {
            if self.grab_building == 0 {
                break;
            }
            let mut selected = None;
            for _ in 0..self.grabs.len() {
                let slot = self.grab_turn;
                self.grab_turn = (slot + 1) % self.grabs.len();
                self.work.jobs_scanned = self.work.jobs_scanned.saturating_add(1);
                if self.grabs[slot]
                    .work
                    .is_some_and(|work| !work.ready && work.failure.is_none())
                {
                    selected = Some(slot);
                    break;
                }
            }
            let slot = selected.expect("counted preparation");
            self.remaining -= 1;
            self.work.edit_steps += 1;
            let mut work = self.grabs[slot].work.unwrap();
            let result = (|| {
                let id = work.midi.ok_or(Error::InvalidState)?;
                if io.progress_indexed_midi_grab(id, 256)? == indexed_midi::Status::Ready {
                    work.take.midi = Some(io.finish_indexed_midi_grab(id)?);
                    work.midi = None;
                    work.ready = true;
                }
                Ok(())
            })();
            if let Err(error) = result {
                work.failure = Some(error);
            }
            if work.ready || work.failure.is_some() {
                self.grab_building -= 1;
            }
            self.track(work.take);
            self.grabs[slot].work = Some(work);
        }
    }
    fn release_grab_slot(&mut self, id: GrabWorkId) {
        let slot = &mut self.grabs[id.slot];
        slot.work = None;
        slot.generation = slot.generation.checked_add(1).unwrap_or(0);
    }
    pub(in crate::runtime) fn grab_result(
        &mut self,
        id: GrabWorkId,
        io: &mut ResourceIo<'_, S>,
    ) -> Result<Option<SharedTake>, Error> {
        let work = self.grab_work(id)?;
        if let Some(error) = work.failure {
            self.cancel_grab_work(id, io)?;
            return Err(error);
        }
        if !work.ready {
            return Ok(None);
        }
        self.release_grab_slot(id);
        Ok(Some(work.take))
    }
    pub(in crate::runtime) fn cancel_grab_work(
        &mut self,
        id: GrabWorkId,
        io: &mut ResourceIo<'_, S>,
    ) -> Result<(), Error> {
        let work = self.grab_work(id)?;
        if let Some(midi) = work.midi {
            io.cancel_indexed_midi_grab(midi)?;
        }
        for resource in [work.take.audio, work.take.midi].into_iter().flatten() {
            io.remove(resource)?;
            self.owned[resource.slot() as usize] = None;
        }
        if !work.ready && work.failure.is_none() {
            self.grab_building -= 1;
        }
        self.release_grab_slot(id);
        Ok(())
    }
    pub(super) fn cancel_all_grabs(&mut self, io: &mut ResourceIo<'_, S>) -> Result<(), Error> {
        for slot in 0..self.grabs.len() {
            if self.grabs[slot].work.is_some() {
                self.cancel_grab_work(
                    GrabWorkId {
                        scope: self.scope,
                        slot,
                        generation: self.grabs[slot].generation,
                    },
                    io,
                )?;
            }
        }
        Ok(())
    }
}
