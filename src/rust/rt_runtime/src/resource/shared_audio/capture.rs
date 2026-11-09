use super::*;

pub(in crate::resource) struct CaptureWrite {
    write: Write,
    payload: LeaseId,
    export: ExportId,
    extra: usize,
}

impl AudioStore {
    pub(in crate::resource) fn capture_slots(&self, members: usize) -> Result<(), Error> {
        if !self.pages.chunked() || self.closed || self.keep == 0 {
            return Err(Error::InvalidState);
        }
        if self.free_views.len() < members.checked_mul(2).ok_or(Error::Capacity)? {
            return Err(Error::Capacity);
        }
        Ok(())
    }

    pub(in crate::resource) fn reserve_capture_block(
        &mut self,
        io: &mut RenderIo<'_>,
        take: TakeId,
        export: ExportId,
    ) -> Result<CaptureWrite, Error> {
        self.check(io.store)?;
        let mut view = self.view(take.0)?;
        let mut reserved = self.view(export.0)?;
        if !view.writable
            || view.growing.is_some()
            || view.base.is_some()
            || view.mapping.first != 0
            || view.mapping.prefix.is_some()
            || !reserved.reserved
            || reserved.root.is_some()
            || view.frames % self.page_frames != 0
        {
            return Err(Error::InvalidState);
        }
        let expected = view.revision;
        view.revision = expected.checked_next()?;
        let page = view.frames / self.page_frames;
        let frames = view
            .frames
            .checked_add(self.page_frames)
            .ok_or(Error::Capacity)?;
        let credit = view.mapping.credits(frames, self.page_frames);
        let extra = credit.checked_sub(view.credit).ok_or(Error::InvalidState)?;
        if page >= self.marks.len()
            || self.reserved == 0
            || self.pages.available_leases() == 0
            || self.index_credit() < self.depth + 1
            || extra
                .checked_mul(2)
                .is_none_or(|extra| extra > self.debt_limit - self.debt)
        {
            return Err(Error::Capacity);
        }
        let payload = self.pages.reserve_samples(io.store)?;
        self.reserved -= 1;
        let previous_root = view.root;
        debug_assert!(self.find(previous_root, page).is_none());
        view.root = Some(self.update_index(io.store, view.root, page, payload, true));
        let undo_append = (self.flat.is_some() && view.root == previous_root).then_some(page);
        view.frames = frames;
        view.credit = credit;
        self.debt += 2 * extra;
        reserved.credit += extra;
        self.views[export.0.slot as usize].value = Some(reserved);
        io.store
            .charge_work(WorkClass::Render, 0, 2 * (self.depth + 1))?;
        Ok(CaptureWrite {
            write: Write {
                undo_append,
                take,
                expected,
                view,
            },
            payload,
            export,
            extra,
        })
    }

    pub(in crate::resource) fn fill_capture_block(
        &mut self,
        io: &mut RenderIo<'_>,
        write: &CaptureWrite,
        samples: &[f32],
    ) -> Result<(), Error> {
        self.check(io.store)?;
        if samples.len() != self.page_frames * self.channels {
            return Err(Error::InvalidShape);
        }
        let previous = self.last_payload.filter(|&lease| {
            self.flat.is_none()
                && self.pages.sample_span(lease).is_ok_and(|previous| {
                    previous
                        .iter()
                        .zip(samples)
                        .all(|(a, b)| a.to_bits() == b.to_bits())
                })
        });
        let shared = if let Some(previous) = previous {
            self.pages
                .share_reserved(io.store, write.payload, previous)?
        } else {
            false
        };
        if shared {
            self.reserved += 1;
            self.work.shared_payloads += 1;
        } else {
            self.pages.fill_samples(io.store, write.payload, samples)?;
        }
        self.last_payload = Some(write.payload);
        Ok(())
    }

    pub(in crate::resource) fn fill_capture_reference(
        &mut self,
        io: &mut RenderIo<'_>,
        write: &CaptureWrite,
        source: LeaseId,
    ) -> Result<(), Error> {
        self.check(io.store)?;
        if !self.pages.share_reserved(io.store, write.payload, source)? {
            return Err(Error::Exhausted);
        }
        self.reserved += 1;
        self.work.shared_payloads += 1;
        Ok(())
    }

    pub(in crate::resource) fn cancel_capture_block(
        &mut self,
        io: &mut RenderIo<'_>,
        write: CaptureWrite,
    ) -> Result<(), Error> {
        let mut export = self.view(write.export.0)?;
        export.credit -= write.extra;
        self.debt -= 2 * write.extra;
        self.views[write.export.0.slot as usize].value = Some(export);
        self.cancel_write(io, write.write)
    }

    pub(in crate::resource) fn commit_capture_block(
        &mut self,
        io: &mut RenderIo<'_>,
        write: CaptureWrite,
    ) -> Result<(), Error> {
        self.commit_write(io, write.write).map(|_| ())
    }
}
