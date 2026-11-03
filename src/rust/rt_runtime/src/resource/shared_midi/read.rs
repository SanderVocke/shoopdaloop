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
        self.block_read(io.store, r.block, r.offset, &mut output[..r.len])?;
        Ok(r.len)
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
