use super::*;
use profiling_protocol::{Action, Descriptor, Name, Outcome, Request, Status};

pub(super) fn broker(epoch: u64) -> profiling_protocol::broker::Broker {
    let mut broker = profiling_protocol::broker::Broker::default();
    broker
        .register(
            Descriptor {
                stream: profiling_data::wire::stream(epoch, profiling_data::wire::VIEW_TYPE),
                name: Name::new("audio.graph.view").expect("static name"),
                available: true,
                min_interval_ms: 200,
                max_interval_ms: 200,
                snapshot: true,
                inspect: true,
            },
            None,
        )
        .expect("prepared audio source");
    broker
}
impl Service {
    pub fn remove_profiling_provider(&mut self, source: u32) -> Result<()> {
        if source == profiling_data::wire::SOURCE {
            return Err(anyhow!("the backend owns the audio source"));
        }
        self.diagnostics
            .remove(source)
            .map_err(|_| anyhow!("unknown or exhausted profiling source"))
    }
    pub fn register_profiling_provider(
        &mut self,
        provider: Box<dyn profiling_protocol::broker::Provider>,
    ) -> Result<()> {
        self.diagnostics
            .register(provider.descriptor(), Some(provider))
            .map_err(|_| anyhow!("profiling source capacity or identity"))
    }
    pub fn take_diagnostic(&mut self, connection: ConnectionId) -> Option<ServiceMessage> {
        let lease = self.lease.as_ref().filter(|l| l.connection == connection)?;
        let authority = lease.authority;
        self.diagnostics
            .next_record()
            .map(|record| ServiceMessage::Diagnostic { authority, record })
    }
    pub(super) fn control_profiling(&mut self, request: Request) -> Result<()> {
        if !request.valid() {
            return Err(anyhow!("invalid profiling request"));
        }
        if matches!(request.action, Action::Discover) {
            self.diagnostics.discover();
            return Ok(());
        }
        let descriptor = self
            .diagnostics
            .descriptor(request.source)
            .ok_or_else(|| anyhow!("unknown profiling source"))?;
        if request.epoch != descriptor.stream.epoch {
            return Err(anyhow!("stale profiling source"));
        }
        if request.source == profiling_data::wire::SOURCE {
            match request.action {
                Action::Subscribe { enabled, .. } => self.backend.set_profiling(enabled)?,
                Action::Snapshot => {}
                Action::Inspect(payload) => {
                    let capture: profiling_data::CaptureRequest =
                        profiling_protocol::decode(payload.as_slice())
                            .map_err(|_| anyhow!("invalid capture request"))?;
                    if capture.capture == 0
                        || capture.generation == 0
                        || capture.offset as usize > profiling_data::MAX_SAMPLES
                    {
                        return Err(anyhow!("invalid capture selection"));
                    }
                    self.backend.inspect_profiling(capture)?;
                }
                Action::Discover => {}
            }
            self.profiling_elapsed = Duration::from_millis(200);
        } else {
            let status = self
                .diagnostics
                .control(request)
                .map_err(|_| anyhow!("invalid profiling source control"))?;
            if status.outcome != Outcome::Applied {
                return Err(anyhow!("profiling control refused: {:?}", status.outcome));
            }
        }
        Ok(())
    }
    pub(super) fn advance_profiling(&mut self, elapsed: Duration) {
        self.profiling_elapsed = self.profiling_elapsed.saturating_add(elapsed);
        if self.lease.as_ref().is_none_or(|l| !l.ready)
            || self.profiling_elapsed < Duration::from_millis(200)
        {
            return;
        }
        self.profiling_elapsed = Duration::ZERO;
        let view = self.audio_profiling_view();
        let Some(mut descriptor) = self.diagnostics.descriptor(profiling_data::wire::SOURCE) else {
            return;
        };
        if self.profiling_connection != view.connection {
            let Some(epoch) = descriptor.stream.epoch.checked_add(1) else {
                return;
            };
            descriptor.stream.epoch = epoch.max(view.connection);
            self.profiling_connection = view.connection;
        }
        let _ = self.diagnostics.update_descriptor(descriptor);
        let _ = self.diagnostics.set_status(Status {
            stream: descriptor.stream,
            requested: view.requested,
            active: view.active,
            interval_ms: 200,
            outcome: if view.requested && !view.available {
                Outcome::Unavailable
            } else {
                Outcome::Applied
            },
            dropped: 0,
            coalesced: 0,
        });
        if view.valid() {
            match profiling_protocol::encode_report(&view) {
                Ok(bytes) => {
                    let _ = self
                        .diagnostics
                        .publish(profiling_data::wire::SOURCE, bytes);
                }
                Err(_) => self
                    .diagnostics
                    .report_failed(profiling_data::wire::SOURCE, Outcome::Capacity),
            }
        }
    }
}
