//! Planner-only prepared capture boundary authority.
use super::*;
use crate::Sample;
use crate::{buffer::MidiBuffer, midi_state::MidiState};
use core::ops::{Deref, DerefMut};

pub struct PlannerResourceIo<'a, S: Sample = f32> {
    resources: ResourceIo<'a, S>,
    frame: Frame,
    ceiling: Frame,
}
impl<'a, S: Sample> Deref for PlannerResourceIo<'a, S> {
    type Target = ResourceIo<'a, S>;
    fn deref(&self) -> &Self::Target {
        &self.resources
    }
}
impl<S: Sample> DerefMut for PlannerResourceIo<'_, S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.resources
    }
}
pub struct CaptureBoundaryIo<'a, S: Sample = f32> {
    store: &'a mut ResourceStore<S>,
    frame: Frame,
    ceiling: Frame,
}

/// A capture take whose source cut precedes its release boundary by a fixed delay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DelayedCaptureTake {
    take: CaptureTakeId,
    delay_frames: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DelayedCaptureTick {
    interval: CaptureTickId,
    delay_frames: usize,
}

impl DelayedCaptureTick {
    pub fn interval(self) -> CaptureTickId {
        self.interval
    }
}
impl<S: Sample> ResourceStore<S> {
    pub fn planner_io(&mut self, frame: Frame, ceiling: Frame) -> PlannerResourceIo<'_, S> {
        PlannerResourceIo {
            resources: self.io(),
            frame,
            ceiling,
        }
    }
}
impl<S: Sample> PlannerResourceIo<'_, S> {
    pub fn capture(&mut self) -> CaptureBoundaryIo<'_, S> {
        CaptureBoundaryIo {
            store: self.resources.store,
            frame: self.frame,
            ceiling: self.ceiling,
        }
    }
}
impl<S: Sample> CaptureBoundaryIo<'_, S> {
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
    /// Start capture at the current source boundary. The first writable interval
    /// is reserved at its corresponding future render boundary.
    pub fn activate_delayed(
        &mut self,
        take: CaptureTakeId,
        delay_frames: usize,
    ) -> Result<DelayedCaptureTake, Error> {
        let quantum = self.store.quantum.frames();
        if delay_frames == 0 || delay_frames % quantum != 0 {
            return Err(Error::InvalidConfig);
        }
        self.frame.checked_add(delay_frames)?.checked_add(quantum)?;
        self.activate(take, self.frame)?;
        Ok(DelayedCaptureTake { take, delay_frames })
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
    /// Reserve the next contiguous source interval at its physical render boundary.
    pub fn reserve_delayed(
        &mut self,
        take: DelayedCaptureTake,
    ) -> Result<DelayedCaptureTick, Error> {
        if self.frame.checked_add(self.store.quantum.frames())? > self.ceiling {
            return Err(Error::InvalidFrame);
        }
        let source = self.source_boundary(take.delay_frames)?;
        let interval = self.store.reserve_capture_tick(take.take, source)?;
        Ok(DelayedCaptureTick {
            interval,
            delay_frames: take.delay_frames,
        })
    }
    pub fn cancel_tick(&mut self, id: CaptureTickId) -> Result<(), Error> {
        self.store.cancel_capture_tick(id)
    }
    pub fn commit(&mut self, id: CaptureTickId) -> Result<(), Error> {
        self.check_frame(self.store.capture_tick_end(id)?)?;
        self.store.commit_capture_tick(id)
    }
    pub fn commit_delayed(&mut self, tick: DelayedCaptureTick) -> Result<(), Error> {
        let source = self.source_boundary(tick.delay_frames)?;
        if self.store.capture_tick_end(tick.interval)? != source {
            return Err(Error::InvalidFrame);
        }
        self.store.commit_capture_tick(tick.interval)
    }
    /// Freeze the complete take at its source cut after its physical tail settles.
    pub fn freeze_delayed(
        &mut self,
        take: DelayedCaptureTake,
        source_cut: Frame,
        output: &mut [Option<Snapshot>],
    ) -> Result<(), Error> {
        if self.source_boundary(take.delay_frames)? != source_cut {
            return Err(Error::InvalidFrame);
        }
        self.store
            .freeze_capture_take(take.take, source_cut, None, output)
    }
    fn source_boundary(&self, delay_frames: usize) -> Result<Frame, Error> {
        self.check_frame(self.frame)?;
        self.frame
            .0
            .checked_sub(delay_frames as u64)
            .map(Frame)
            .ok_or(Error::InvalidFrame)
    }
    fn check_frame(&self, frame: Frame) -> Result<(), Error> {
        if frame != self.frame || self.frame > self.ceiling {
            return Err(Error::InvalidFrame);
        }
        Ok(())
    }
}
impl<S: Sample> RenderIo<'_, S> {
    pub fn stage_audio(
        &mut self,
        interval: CaptureTickId,
        member: usize,
        samples: &[S],
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
