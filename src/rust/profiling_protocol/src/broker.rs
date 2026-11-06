use crate::*;
use alloc::{boxed::Box, vec::Vec};
use core::time::Duration;

pub trait Provider {
    fn descriptor(&self) -> Descriptor;
    fn control(&mut self, action: Action) -> Status;
    fn advance(&mut self, elapsed: Duration);
    fn take_report(&mut self) -> Option<Result<Vec<u8>, Outcome>>;
}
struct Source {
    descriptor: Descriptor,
    status: Status,
    catalog_pending: bool,
    status_pending: bool,
    provider: Option<Box<dyn Provider>>,
    active: Option<Transfer>,
    pending: Option<Vec<u8>>,
    sequence: u64,
    removed: bool,
}
#[derive(Default)]
pub struct Broker {
    sources: Vec<Source>,
    cursor: usize,
}
impl Broker {
    pub fn register(
        &mut self,
        descriptor: Descriptor,
        provider: Option<Box<dyn Provider>>,
    ) -> Result<(), Error> {
        if !descriptor.valid()
            || self
                .sources
                .iter()
                .any(|s| s.descriptor.stream.source == descriptor.stream.source)
        {
            return Err(Error::Invalid);
        }
        if self.sources.len() == MAX_SOURCES {
            return Err(Error::Capacity);
        }
        self.sources.push(Source {
            descriptor,
            status: Status {
                stream: descriptor.stream,
                requested: false,
                active: false,
                interval_ms: descriptor.min_interval_ms,
                outcome: Outcome::Applied,
                dropped: 0,
                coalesced: 0,
            },
            catalog_pending: true,
            status_pending: true,
            provider,
            active: None,
            pending: None,
            sequence: 0,
            removed: false,
        });
        Ok(())
    }
    pub fn descriptor(&self, source: u32) -> Option<Descriptor> {
        self.sources
            .iter()
            .find(|s| s.descriptor.stream.source == source)
            .map(|s| s.descriptor)
    }
    pub fn control(&mut self, request: Request) -> Result<Status, Error> {
        if !request.valid() {
            return Err(Error::Invalid);
        }
        if matches!(request.action, Action::Discover) {
            for source in &mut self.sources {
                source.catalog_pending = true;
                source.status_pending = true;
            }
            return self.sources.first().map(|s| s.status).ok_or(Error::Invalid);
        }
        let source = self
            .sources
            .iter_mut()
            .find(|s| s.descriptor.stream.source == request.source)
            .ok_or(Error::Invalid)?;
        if source.removed {
            return Err(Error::Invalid);
        }
        if source.descriptor.stream.epoch != request.epoch {
            return Err(Error::Stale);
        }
        if let Some(provider) = &mut source.provider {
            let status = provider.control(request.action);
            if status.stream != source.descriptor.stream {
                return Err(Error::Invalid);
            }
            source.status = Status {
                dropped: source.status.dropped,
                coalesced: source.status.coalesced,
                ..status
            };
        } else {
            source.status.outcome = Outcome::Unsupported;
        }
        source.status_pending = true;
        Ok(source.status)
    }
    pub fn set_status(&mut self, status: Status) -> Result<(), Error> {
        let source = self
            .sources
            .iter_mut()
            .find(|s| s.descriptor.stream == status.stream)
            .ok_or(Error::Invalid)?;
        let next = Status {
            dropped: source.status.dropped,
            coalesced: source.status.coalesced,
            ..status
        };
        source.status_pending |= source.status != next;
        source.status = next;
        Ok(())
    }
    pub fn publish(&mut self, id: u32, bytes: Vec<u8>) -> Result<(), Error> {
        let retained: usize = self
            .sources
            .iter()
            .map(|s| {
                s.active.as_ref().map_or(0, Transfer::bytes)
                    + s.pending.as_ref().map_or(0, Vec::capacity)
            })
            .sum();
        let source = self
            .sources
            .iter_mut()
            .find(|s| s.descriptor.stream.source == id)
            .ok_or(Error::Invalid)?;
        if source.removed {
            return Err(Error::Invalid);
        }
        if bytes.is_empty()
            || bytes.len() > MAX_REPORT_BYTES
            || retained - source.pending.as_ref().map_or(0, Vec::capacity) + bytes.capacity()
                > MAX_REPORT_BYTES
        {
            source.status.dropped = source.status.dropped.saturating_add(1);
            source.status.outcome = Outcome::Capacity;
            source.status_pending = true;
            return Err(Error::Capacity);
        }
        if source.pending.is_some() {
            source.status.coalesced = source.status.coalesced.saturating_add(1);
            source.status_pending = true;
        }
        if source.status.outcome == Outcome::Capacity {
            source.status.outcome = Outcome::Applied;
            source.status_pending = true;
        }
        source.pending = Some(bytes);
        Ok(())
    }
    pub fn advance(&mut self, elapsed: Duration) {
        for index in 0..self.sources.len() {
            let report = if let Some(provider) = &mut self.sources[index].provider {
                provider.advance(elapsed);
                provider.take_report()
            } else {
                None
            };
            if let Some(report) = report {
                let id = self.sources[index].descriptor.stream.source;
                match report {
                    Ok(bytes) => {
                        let _ = self.publish(id, bytes);
                    }
                    Err(outcome) => self.report_failed(id, outcome),
                }
            }
        }
    }
    pub fn report_failed(&mut self, id: u32, outcome: Outcome) {
        if let Some(source) = self
            .sources
            .iter_mut()
            .find(|s| s.descriptor.stream.source == id)
        {
            source.status.dropped = source.status.dropped.saturating_add(1);
            source.status.outcome = outcome;
            source.status_pending = true;
        }
    }
    pub fn next_record(&mut self) -> Option<Record> {
        for _ in 0..self.sources.len() {
            let index = self.cursor % self.sources.len();
            self.cursor = (index + 1) % self.sources.len();
            let source = &mut self.sources[index];
            if source.catalog_pending {
                source.catalog_pending = false;
                return Some(Record::Descriptor(source.descriptor));
            }
            if source.status_pending {
                source.status_pending = false;
                return Some(Record::Status(source.status));
            }
            if source.active.is_none() {
                if let Some(bytes) = source.pending.take() {
                    let Some(sequence) = source.sequence.checked_add(1) else {
                        source.status.outcome = Outcome::Capacity;
                        source.status_pending = true;
                        continue;
                    };
                    source.sequence = sequence;
                    source.active = Transfer::new(source.descriptor.stream, sequence, bytes).ok();
                }
            }
            if let Some(active) = &mut source.active {
                if let Some(chunk) = active.next_chunk() {
                    return Some(Record::Data(chunk));
                }
                source.active = None;
            }
        }
        None
    }
    pub fn detach(&mut self) {
        for source in &mut self.sources {
            if let Some(provider) = &mut source.provider {
                source.status = provider.control(Action::Subscribe {
                    enabled: false,
                    interval_ms: source.status.interval_ms,
                });
            }
            source.active = None;
            source.pending = None;
            source.status.active = false;
            source.status.requested = false;
            source.catalog_pending = true;
            source.status_pending = true;
        }
    }
    pub fn discover(&mut self) {
        for s in &mut self.sources {
            s.catalog_pending = true;
            s.status_pending = true;
        }
    }
}
impl Broker {
    pub fn update_descriptor(&mut self, descriptor: Descriptor) -> Result<(), Error> {
        if !descriptor.valid() {
            return Err(Error::Invalid);
        }
        let source = self
            .sources
            .iter_mut()
            .find(|s| s.descriptor.stream.source == descriptor.stream.source)
            .ok_or(Error::Invalid)?;
        if descriptor.stream.epoch < source.descriptor.stream.epoch {
            return Err(Error::Stale);
        }
        if source.descriptor.stream != descriptor.stream {
            source.active = None;
            source.pending = None;
            source.sequence = 0;
            source.status.stream = descriptor.stream;
            source.catalog_pending = true;
            source.status_pending = true;
        }
        source.descriptor = descriptor;
        Ok(())
    }
}

impl Broker {
    pub fn remove(&mut self, id: u32) -> Result<(), Error> {
        let source = self
            .sources
            .iter_mut()
            .find(|s| s.descriptor.stream.source == id)
            .ok_or(Error::Invalid)?;
        let epoch = source
            .descriptor
            .stream
            .epoch
            .checked_add(1)
            .ok_or(Error::Capacity)?;
        if let Some(provider) = &mut source.provider {
            let _ = provider.control(Action::Subscribe {
                enabled: false,
                interval_ms: source.status.interval_ms,
            });
        }
        source.provider = None;
        source.active = None;
        source.pending = None;
        source.removed = true;
        source.descriptor.stream.epoch = epoch;
        source.descriptor.available = false;
        source.status.stream = source.descriptor.stream;
        source.status.active = false;
        source.status.requested = false;
        source.status.outcome = Outcome::Unavailable;
        source.catalog_pending = true;
        source.status_pending = true;
        Ok(())
    }
}
