//! Required data access uses prepared storage, not background transfer credits.

use super::*;
use crate::buffer::MidiBuffer;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RenderWork {
    pub operations: usize,
    pub copied_bytes: usize,
    pub validation_steps: usize,
    pub counter_overflow: bool,
}
impl RenderWork {
    fn add(&mut self, operations: usize, bytes: usize, steps: usize) {
        for (counter, amount) in [
            (&mut self.operations, operations),
            (&mut self.copied_bytes, bytes),
            (&mut self.validation_steps, steps),
        ] {
            match counter.checked_add(amount) {
                Some(value) => *counter = value,
                None => {
                    *counter = usize::MAX;
                    self.counter_overflow = true;
                }
            }
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum WorkClass {
    Background,
    Render,
}

/// No upload, structural mutation, accounting reset, or reclamation authority.
/// Data access still validates identities, shapes, timestamps, and fixed capacity.
pub struct RenderIo<'a> {
    pub(super) store: &'a mut ResourceStore,
}
impl RenderIo<'_> {
    /// Metadata for the pinned prefix, not the resource's newer active version.
    pub fn snapshot(&mut self, index: ReadIndex) -> Result<Snapshot, Error> {
        self.store.note_operation(WorkClass::Render);
        Ok(self.store.index_lease(index)?.snapshot)
    }
    pub fn cursor_at(&mut self, index: ReadIndex, offset: usize) -> Result<Cursor, Error> {
        self.store.cursor_at_with(index, offset, WorkClass::Render)
    }
    pub fn read(&mut self, cursor: &mut Cursor, output: &mut [u8]) -> Result<usize, Error> {
        self.store.read_with(cursor, output, WorkClass::Render)
    }
    pub fn capture_audio(
        &mut self,
        capture: CaptureId,
        frame: Frame,
        samples: &[f32],
    ) -> Result<Revision, Error> {
        self.store
            .capture_audio_with(capture, frame, samples, WorkClass::Render)
    }
    pub fn capture_midi(
        &mut self,
        capture: CaptureId,
        frame: Frame,
        frames: usize,
        input: &MidiBuffer,
    ) -> Result<Revision, Error> {
        self.store
            .capture_midi_with(capture, frame, frames, input, WorkClass::Render)
    }
}
impl ResourceStore {
    pub fn render_io(&mut self) -> RenderIo<'_> {
        RenderIo { store: self }
    }
    pub fn render_work(&self) -> RenderWork {
        self.render_work
    }
    pub(super) fn note_operation(&mut self, class: WorkClass) {
        if class == WorkClass::Render {
            self.render_work.add(1, 0, 0);
        }
    }
    pub(super) fn charge_work(
        &mut self,
        class: WorkClass,
        bytes: usize,
        steps: usize,
    ) -> Result<(), Error> {
        match class {
            WorkClass::Background => self.charge_external(bytes, steps),
            WorkClass::Render => {
                self.render_work.add(0, bytes, steps);
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[shoop_wasm_test_support::shoop_test]
    fn render_work_saturation_never_changes_the_access_result() {
        let mut store = ResourceStore::new(crate::Config::default().prepare().unwrap()).unwrap();
        store.render_work = RenderWork {
            operations: usize::MAX,
            copied_bytes: usize::MAX - 1,
            validation_steps: usize::MAX,
            counter_overflow: false,
        };
        store.note_operation(WorkClass::Render);
        assert_eq!(store.charge_work(WorkClass::Render, 2, 1), Ok(()));
        assert_eq!(
            store.render_work,
            RenderWork {
                operations: usize::MAX,
                copied_bytes: usize::MAX,
                validation_steps: usize::MAX,
                counter_overflow: true
            }
        );
        assert_eq!(store.work(), Work::default());
    }
}
