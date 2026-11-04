//! Native driver graph adapted to the portable session service.
#[cfg(feature = "jack")]
mod jack {
    use shoop_app::{HostGraphController, HostGraphError};
    use shoop_app_api::*;
    use shoop_app_protocol::Refusal;
    use shoop_rt_host_native::graph;
    use std::sync::Arc;

    const _: () = assert!(graph::MAX_PORTS == MAX_HOST_GRAPH_PORTS);
    const _: () = assert!(graph::MAX_LINKS == MAX_HOST_GRAPH_LINKS);

    pub(super) struct Controller {
        pub control: graph::Control,
        pub cached: Arc<HostGraphState>,
    }
    fn error(error: graph::Error) -> HostGraphError {
        let reason = match &error {
            graph::Error::Unavailable => Refusal::NotReady,
            graph::Error::Stale => Refusal::RevisionConflict,
            graph::Error::Invalid => Refusal::Invalid,
            graph::Error::Busy => Refusal::Overloaded,
            graph::Error::Failed(_) => Refusal::Engine,
        };
        HostGraphError {
            reason,
            message: error.to_string(),
        }
    }
    impl HostGraphController for Controller {
        fn snapshot(&mut self) -> Arc<HostGraphState> {
            let snapshot = self.control.snapshot();
            if snapshot.epoch != self.cached.epoch || snapshot.revision != self.cached.revision {
                self.cached = Arc::new(HostGraphState {
                    epoch: snapshot.epoch,
                    revision: snapshot.revision,
                    available: snapshot.available,
                    error: snapshot.error.clone(),
                    ports: snapshot
                        .ports
                        .iter()
                        .map(|port| HostGraphPort {
                            name: port.name.clone(),
                            data_type: match port.data_type {
                                graph::DataType::Audio => PortDataType::Audio,
                                graph::DataType::Midi => PortDataType::Midi,
                            },
                            direction: match port.direction {
                                graph::Direction::Input => PortDirection::Input,
                                graph::Direction::Output => PortDirection::Output,
                            },
                            owned: port.owned,
                        })
                        .collect(),
                    links: snapshot
                        .links
                        .iter()
                        .map(|link| HostGraphLink {
                            source: link.source.clone(),
                            destination: link.destination.clone(),
                        })
                        .collect(),
                });
            }
            self.cached.clone()
        }
        fn submit(&mut self, request: HostGraphRequest) -> Result<u64, HostGraphError> {
            self.control
                .submit(graph::Request {
                    epoch: request.epoch,
                    revision: request.revision,
                    local: request.local,
                    peer: request.peer,
                    connected: request.connected,
                })
                .map_err(error)
        }
        fn take_result(&mut self) -> Option<(u64, Result<(), HostGraphError>)> {
            self.control
                .take_result()
                .map(|(id, result)| (id, result.map_err(error)))
        }
    }
}

pub fn controller(
    host: &shoop_rt_host_native::NativeHost,
) -> Option<Box<dyn shoop_app::HostGraphController>> {
    #[cfg(feature = "jack")]
    if let Some(control) = host.graph_control() {
        return Some(Box::new(jack::Controller {
            control,
            cached: Default::default(),
        }));
    }
    #[cfg(not(feature = "jack"))]
    let _ = host;
    None
}
