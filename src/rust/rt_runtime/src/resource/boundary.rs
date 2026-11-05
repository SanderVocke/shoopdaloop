//! Planner-only prepared capture boundary authority.
use super::*;
use crate::{buffer::MidiBuffer, midi_state::MidiState};
use core::ops::{Deref, DerefMut};

pub struct PlannerResourceIo<'a> {
    resources: ResourceIo<'a>,
    frame: Frame,
    ceiling: Frame,
}
impl<'a> Deref for PlannerResourceIo<'a> {
    type Target = ResourceIo<'a>;
    fn deref(&self) -> &Self::Target {
        &self.resources
    }
}
impl DerefMut for PlannerResourceIo<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.resources
    }
}
pub struct CaptureBoundaryIo<'a> {
    store: &'a mut ResourceStore,
    frame: Frame,
    ceiling: Frame,
}
impl ResourceStore {
    pub fn planner_io(&mut self, frame: Frame, ceiling: Frame) -> PlannerResourceIo<'_> {
        PlannerResourceIo {
            resources: self.io(),
            frame,
            ceiling,
        }
    }
}
impl PlannerResourceIo<'_> {
    pub fn capture(&mut self) -> CaptureBoundaryIo<'_> {
        CaptureBoundaryIo {
            store: self.resources.store,
            frame: self.frame,
            ceiling: self.ceiling,
        }
    }
}
impl CaptureBoundaryIo<'_> {
    pub fn check_idle_group(&self, id: CaptureGroupId) -> Result<(), Error> {
        self.store.check_idle_capture_group(id)
    }
    pub fn clear_idle_group(&mut self, id: CaptureGroupId) -> Result<(), Error> {
        self.store.clear_idle_capture_group(id)
    }
    pub fn release_group(&mut self, id: CaptureGroupId) -> Result<(), Error> {
        self.store.release_capture_group(id)
    }
    pub fn info(&self, id: CaptureGroupId) -> Result<CaptureGroupInfo, Error> {
        self.store.capture_group_info(id)
    }
    pub fn prepare(
        &mut self,
        id: CaptureGroupId,
        initial: &[MidiState],
    ) -> Result<CaptureTakeId, Error> {
        self.store.prepare_capture_take(id, initial)
    }
    pub fn cancel_take(&mut self, id: CaptureTakeId) -> Result<(), Error> {
        self.store.cancel_capture_take(id)
    }
    pub fn abort(&mut self, id: CaptureTakeId) -> Result<(), Error> {
        self.store.abort_capture_take(id)
    }
    pub fn activate(&mut self, id: CaptureTakeId, frame: Frame) -> Result<(), Error> {
        self.check_frame(frame)?;
        self.store.activate_capture_take(id, frame)
    }
    pub fn freeze(
        &mut self,
        id: CaptureTakeId,
        frame: Frame,
        incomplete: Option<Error>,
        output: &mut [Option<Snapshot>],
    ) -> Result<(), Error> {
        self.check_frame(frame)?;
        self.store
            .freeze_capture_take(id, frame, incomplete, output)
    }
    /// Reserve an unpublished take's first interval before activation.
    pub fn reserve_prepared(
        &mut self,
        id: CaptureTakeId,
        frame: Frame,
    ) -> Result<CaptureTickId, Error> {
        self.check_frame(frame)?;
        if frame.checked_add(self.store.quantum.frames())? > self.ceiling {
            return Err(Error::InvalidFrame);
        }
        self.store.reserve_prepared_capture_tick(id, frame)
    }
    pub fn reserve(&mut self, id: CaptureTakeId, frame: Frame) -> Result<CaptureTickId, Error> {
        self.check_frame(frame)?;
        if frame.checked_add(self.store.quantum.frames())? > self.ceiling {
            return Err(Error::InvalidFrame);
        }
        self.store.reserve_capture_tick(id, frame)
    }
    pub fn cancel_tick(&mut self, id: CaptureTickId) -> Result<(), Error> {
        self.store.cancel_capture_tick(id)
    }
    pub fn commit(&mut self, id: CaptureTickId) -> Result<(), Error> {
        self.check_frame(self.store.capture_tick_end(id)?)?;
        self.store.commit_capture_tick(id)
    }
    fn check_frame(&self, frame: Frame) -> Result<(), Error> {
        if frame != self.frame || self.frame > self.ceiling {
            return Err(Error::InvalidFrame);
        }
        Ok(())
    }
}
impl RenderIo<'_> {
    pub fn stage_audio(
        &mut self,
        interval: CaptureTickId,
        member: usize,
        samples: &[f32],
    ) -> Result<(), Error> {
        self.store.stage_capture_audio(interval, member, samples)
    }
    pub fn stage_midi(
        &mut self,
        interval: CaptureTickId,
        member: usize,
        input: &MidiBuffer,
    ) -> Result<(), Error> {
        self.store.stage_capture_midi(interval, member, input)
    }
}
