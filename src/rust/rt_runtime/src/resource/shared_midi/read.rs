use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cursor {
    index: ReadIndex,
    revision: Revision,
    point: Point,
    offset: usize,
}
impl Cursor {
    pub fn byte_position(self) -> usize {
        self.offset
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Event {
    index: ReadIndex,
    revision: Revision,
    point: Point,
    frame: u64,
    bytes: usize,
}
impl Event {
    pub fn frame(self) -> u64 {
        self.frame
    }
    pub fn byte_len(self) -> usize {
        self.bytes
    }
}
pub struct Position {
    pub cursor: Cursor,
    pub state: MidiState,
    pub replayed_events: usize,
}

impl MidiStore {
    pub(in crate::resource) fn read_bytes(
        &self,
        store: &mut ResourceStore,
        index: ReadIndex,
        offset: usize,
        output: &mut [u8],
        class: WorkClass,
    ) -> Result<usize, Error> {
        self.check(store)?;
        let view = self.view(index.0)?;
        if offset > view.bytes {
            return Err(Error::InvalidOffset);
        }
        let count = output.len().min(self.page_bytes).min(view.bytes - offset);
        store.charge_work(class, count, 0)?;
        let mut copied = 0;
        while copied < count {
            let step = self.read_piece(
                store,
                index,
                offset + copied,
                &mut output[copied..count],
                class,
            )?;
            if step == 0 {
                return Err(Error::InvalidState);
            }
            copied += step;
        }
        Ok(count)
    }
    fn read_piece(
        &self,
        store: &mut ResourceStore,
        index: ReadIndex,
        offset: usize,
        output: &mut [u8],
        class: WorkClass,
    ) -> Result<usize, Error> {
        self.check(store)?;
        let view = self.view(index.0)?;
        if offset > view.bytes {
            return Err(Error::InvalidOffset);
        }
        if offset == view.bytes || output.is_empty() {
            return Ok(0);
        }
        let end = view.last.serial + u64::from(view.last.event > 0);
        let mut low = view.first.serial;
        let mut high = end;
        while low < high {
            let mid = low + (high - low) / 2;
            let after = if mid + 1 == end {
                view.last
            } else {
                Point {
                    serial: mid + 1,
                    event: 0,
                }
            };
            if self.between(view.root, view.first, after)?.0 <= offset {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        let first = Point {
            serial: low,
            event: if low == view.first.serial {
                view.first.event
            } else {
                0
            },
        };
        let mut local = offset - self.between(view.root, view.first, first)?.0;
        let chunk = self.chunks[self.find(view.root, low)?].expect("byte chunk");
        let limit = if low == view.last.serial {
            view.last.event
        } else {
            chunk.count
        };
        for record in &chunk.records[first.event..limit] {
            if local >= 12 + record.len {
                local -= 12 + record.len;
                continue;
            }
            let count = output
                .len()
                .min(if local < 12 {
                    12 - local
                } else {
                    12 + record.len - local
                })
                .min(self.page_bytes);
            let index_steps = 4 * (self.depth + 1) * (2 * self.depth + 3);
            store.charge_work(
                class,
                0,
                if class == WorkClass::Render {
                    index_steps
                } else {
                    0
                },
            )?;
            for _ in 0..index_steps {
                store.index_step(class);
            }
            if local < 12 {
                let mut header = [0; 12];
                header[..8].copy_from_slice(&(record.frame - view.origin).to_le_bytes());
                header[8..].copy_from_slice(&(record.len as u32).to_le_bytes());
                output[..count].copy_from_slice(&header[local..local + count]);
            } else {
                self.block_read(
                    store,
                    record.block,
                    record.offset + local - 12,
                    &mut output[..count],
                    class,
                )?;
            }
            return Ok(count);
        }
        Err(Error::InvalidState)
    }
    pub(super) fn locate(&self, view: View, time: u64) -> Result<Point, Error> {
        if view.first == view.last {
            return Ok(view.last);
        }
        let end = view.last.serial + u64::from(view.last.event > 0);
        let mut low = view.first.serial;
        let mut high = end;
        while low < high {
            let mid = low + (high - low) / 2;
            let chunk = self.chunks[self.find(view.root, mid)?].expect("indexed chunk");
            let count = if mid == view.last.serial {
                view.last.event
            } else {
                chunk.count
            };
            if chunk.records[count - 1].frame < time {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        if low == end {
            return Ok(view.last);
        }
        let chunk = self.chunks[self.find(view.root, low)?].expect("indexed chunk");
        let start = if low == view.first.serial {
            view.first.event
        } else {
            0
        };
        let end = if low == view.last.serial {
            view.last.event
        } else {
            chunk.count
        };
        for event in start..end {
            if chunk.records[event].frame >= time {
                return Ok(Point { serial: low, event });
            }
        }
        Ok(view.last)
    }
    pub(super) fn state_at(
        &mut self,
        store: &mut ResourceStore,
        view: View,
        time: u64,
    ) -> Result<(MidiState, usize), Error> {
        if view.first == view.last {
            self.work.add(0, 1, 0, 0);
            store.charge_work(WorkClass::Render, size_of::<MidiState>(), 1)?;
            return Ok((*self.current, 0));
        }
        let point = self.locate(view, time)?;
        let (serial, count) = if point.event == 0 && point == view.last {
            (point.serial - 1, None)
        } else {
            (point.serial, Some(point.event))
        };
        let index = self.find(view.root, serial)?;
        let chunk = self.chunks[index].expect("checkpoint chunk");
        let count = count.unwrap_or(chunk.count);
        let mut state = self.chunk_states[index];
        self.work.add(0, 1, 0, 0);
        store.charge_work(WorkClass::Render, size_of::<MidiState>(), 1)?;
        for record in &chunk.records[..count] {
            self.apply_record(store, &mut state, *record)?;
        }
        Ok((state, count))
    }
    pub(super) fn record(&self, view: View, point: Point) -> Result<Record, Error> {
        if point == view.last {
            return Err(Error::InvalidOffset);
        }
        let chunk = self.chunks[self.find(view.root, point.serial)?].expect("live chunk");
        if point.event >= chunk.count {
            return Err(Error::InvalidOffset);
        }
        Ok(chunk.records[point.event])
    }
    pub fn cursor(&self, index: ReadIndex) -> Result<Cursor, Error> {
        let v = self.view(index.0)?;
        Ok(Cursor {
            index,
            revision: v.revision,
            point: v.first,
            offset: 0,
        })
    }
    pub fn next(&self, cursor: &mut Cursor) -> Result<Option<Event>, Error> {
        let v = self.view(cursor.index.0)?;
        if v.revision != cursor.revision {
            return Err(Error::StaleRevision);
        }
        if cursor.point == v.last {
            return Ok(None);
        }
        let point = cursor.point;
        let r = self.record(v, point)?;
        let chunk = self.chunks[self.find(v.root, point.serial)?].expect("live chunk");
        cursor.point = if point.event + 1 == chunk.count {
            Point {
                serial: point.serial + 1,
                event: 0,
            }
        } else {
            Point {
                serial: point.serial,
                event: point.event + 1,
            }
        };
        cursor.offset += 12 + r.len;
        Ok(Some(Event {
            index: cursor.index,
            revision: v.revision,
            point,
            frame: r.frame - v.origin,
            bytes: r.len,
        }))
    }
    pub fn read_event(
        &self,
        io: &mut RenderIo<'_>,
        event: Event,
        output: &mut [u8],
    ) -> Result<usize, Error> {
        self.check(io.store)?;
        let v = self.view(event.index.0)?;
        if v.revision != event.revision {
            return Err(Error::StaleRevision);
        }
        let r = self.record(v, event.point)?;
        if output.len() < r.len {
            return Err(Error::Capacity);
        }
        io.store
            .charge_work(WorkClass::Render, r.len, self.depth + 1)?;
        self.block_read(
            io.store,
            r.block,
            r.offset,
            &mut output[..r.len],
            WorkClass::Render,
        )?;
        Ok(r.len)
    }
    pub fn parameter_history_untracked(
        &mut self,
        io: &mut RenderIo<'_>,
        index: ReadIndex,
        frame: u64,
    ) -> Result<bool, Error> {
        self.check(io.store)?;
        let view = self.view(index.0)?;
        if frame > view.frames {
            return Err(Error::InvalidFrame);
        }
        let history = |state: &MidiState| {
            (0..16).fold(0u16, |mask, channel| {
                mask | (u16::from(
                    state
                        .channel(channel)
                        .expect("MIDI channel")
                        .parameter_history_untracked(),
                ) << channel)
            })
        };
        if frame == 0 || view.first == view.last {
            return Ok(history(&self.initial[index.0.slot as usize]) != 0);
        }
        let point = self.locate(view, view.origin + frame)?;
        let (serial, count) = if point.event == 0 && point == view.last {
            (point.serial - 1, None)
        } else {
            (point.serial, Some(point.event))
        };
        let slot = self.find(view.root, serial)?;
        let chunk = self.chunks[slot].expect("checkpoint chunk");
        let mut mask = history(&self.chunk_states[slot]);
        for record in &chunk.records[..count.unwrap_or(chunk.count)] {
            let mut short = [0; 3];
            let count = record.len.min(3);
            self.block_read(
                io.store,
                record.block,
                record.offset,
                &mut short[..count],
                WorkClass::Render,
            )?;
            if short[0] == 0xff {
                mask = 0;
            } else if matches!(short, [0xb0..=0xbf, 6 | 38 | 96 | 97, _]) {
                mask |= 1 << (short[0] & 15);
            }
            self.work.add(1, 0, 0, 0);
            io.store.charge_work(WorkClass::Render, count, 1)?;
        }
        Ok(mask != 0)
    }
    pub fn seek(
        &mut self,
        io: &mut RenderIo<'_>,
        index: ReadIndex,
        frame: u64,
    ) -> Result<Position, Error> {
        self.check(io.store)?;
        let v = self.view(index.0)?;
        if frame > v.frames {
            return Err(Error::InvalidFrame);
        }
        let point = self.locate(v, v.origin + frame)?;
        let (state, replayed_events) = if frame == 0 || v.first == v.last {
            self.work.add(0, 1, 0, 0);
            io.store
                .charge_work(WorkClass::Render, size_of::<MidiState>(), 1)?;
            (self.initial[index.0.slot as usize], 0)
        } else {
            self.state_at(io.store, v, v.origin + frame)?
        };
        let offset = self.between(v.root, v.first, point)?.0;
        Ok(Position {
            cursor: Cursor {
                index,
                revision: v.revision,
                point,
                offset,
            },
            state,
            replayed_events,
        })
    }
}
