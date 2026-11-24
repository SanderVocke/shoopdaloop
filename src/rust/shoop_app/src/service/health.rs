use alloc::{rc::Rc, vec::Vec};
use core::{cell::Cell, time::Duration};
use profiling_protocol::{broker::Provider, health, Action, Descriptor, Outcome, Status};

pub(super) type Reader = Rc<Cell<fn() -> health::Snapshot>>;

pub(super) fn reader() -> Reader {
    Rc::new(Cell::new(
        health::Snapshot::default as fn() -> health::Snapshot,
    ))
}

pub(super) struct Source {
    reader: Reader,
    status: Status,
    elapsed: Duration,
    force: bool,
    pending: Option<Result<Vec<u8>, Outcome>>,
}

impl Source {
    pub fn new(epoch: u64, reader: Reader) -> Self {
        Self {
            reader,
            status: Status {
                stream: health::stream(epoch),
                requested: false,
                active: false,
                interval_ms: health::INTERVAL_MS,
                outcome: Outcome::Applied,
                dropped: 0,
                coalesced: 0,
            },
            elapsed: Duration::ZERO,
            force: false,
            pending: None,
        }
    }
}

impl Provider for Source {
    fn descriptor(&self) -> Descriptor {
        health::descriptor(self.status.stream.epoch)
    }

    fn control(&mut self, action: Action) -> Status {
        self.status.outcome = Outcome::Applied;
        match action {
            Action::Subscribe {
                enabled,
                interval_ms,
            } => {
                self.status.requested = enabled;
                self.status.active = enabled;
                self.status.interval_ms = interval_ms.clamp(health::INTERVAL_MS, 60000);
                self.elapsed = Duration::ZERO;
                self.force = enabled;
                self.pending = None;
            }
            Action::Snapshot => self.force = true,
            Action::Inspect(_) => self.status.outcome = Outcome::Unsupported,
            Action::Discover => {}
        }
        self.status
    }

    fn advance(&mut self, elapsed: Duration) {
        self.elapsed = self.elapsed.saturating_add(elapsed);
        if !self.force
            && (!self.status.active
                || self.elapsed < Duration::from_millis(u64::from(self.status.interval_ms)))
        {
            return;
        }
        self.elapsed = Duration::ZERO;
        self.force = false;
        let sample = self.reader.get()();
        self.pending = Some(if sample.valid() {
            profiling_protocol::encode_report(&sample).map_err(|_| Outcome::Capacity)
        } else {
            Err(Outcome::Unavailable)
        });
    }

    fn take_report(&mut self) -> Option<Result<Vec<u8>, Outcome>> {
        self.pending.take()
    }
}

impl super::Service {
    /// Install a control-thread sampler before attaching a client. Never called by the renderer.
    pub fn set_health_reader(&mut self, reader: fn() -> health::Snapshot) -> anyhow::Result<()> {
        if self.lease.is_some() {
            return Err(anyhow::anyhow!("configure health before client attachment"));
        }
        self.health_reader.set(reader);
        Ok(())
    }

    pub(super) fn start_health(&mut self) {
        let descriptor = self
            .diagnostics
            .descriptor(health::SOURCE)
            .expect("built-in health source");
        self.diagnostics
            .control(profiling_protocol::Request {
                source: health::SOURCE,
                epoch: descriptor.stream.epoch,
                action: Action::Subscribe {
                    enabled: true,
                    interval_ms: health::INTERVAL_MS,
                },
            })
            .expect("built-in health subscription");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test]
    fn health_collection_is_paced_snapshot_capable_and_stops_when_unsubscribed() {
        let reader = reader();
        let mut source = Source::new(1, reader.clone());
        source.advance(Duration::from_secs(10));
        assert!(source.take_report().is_none());
        let status = source.control(Action::Subscribe {
            enabled: true,
            interval_ms: 1,
        });
        assert_eq!(status.interval_ms, 1000);
        source.advance(Duration::ZERO);
        let bytes = source.take_report().unwrap().unwrap();
        assert_eq!(
            profiling_protocol::decode::<health::Snapshot>(&bytes).unwrap(),
            health::Snapshot::default()
        );
        source.advance(Duration::from_millis(999));
        assert!(source.take_report().is_none());
        source.advance(Duration::from_millis(1));
        assert!(source.take_report().is_some());
        source.advance(Duration::from_secs(10));
        source.control(Action::Subscribe {
            enabled: false,
            interval_ms: 1000,
        });
        assert!(source.take_report().is_none());
        reader.set(|| panic!("inactive sampler must not run"));
        source.advance(Duration::from_secs(10));
        assert!(source.take_report().is_none());
        reader.set(health::Snapshot::default);
        source.control(Action::Snapshot);
        source.advance(Duration::ZERO);
        assert!(source.take_report().is_some());
        source.advance(Duration::from_secs(10));
        assert!(source.take_report().is_none());
    }
}
