use profiling_protocol::{Assembly, Record, MAX_REPORT_BYTES, MAX_SOURCES};
use shoop_app_api::{profiling_data, DiagnosticsView, ProfilingSource, ProfilingView};

#[derive(Default)]
pub struct Store {
    pub audio: ProfilingView,
    pub view: DiagnosticsView,
    assemblies: Vec<Assembly>,
}
impl Store {
    pub fn advance(&mut self, elapsed: core::time::Duration) {
        for source in &mut self.view.sources {
            source.age_ms = source
                .age_ms
                .saturating_add(elapsed.as_millis().min(u128::from(u64::MAX)) as u64);
        }
    }
    pub fn receive(&mut self, record: Record) {
        if !record.valid() {
            return;
        }
        let stream = record.stream();
        let position = self
            .view
            .sources
            .iter()
            .position(|s| s.descriptor.stream.source == stream.source);
        if let Record::Descriptor(descriptor) = record {
            let supported = (stream.source == profiling_data::wire::SOURCE
                && stream.report_type == profiling_data::wire::VIEW_TYPE
                && stream.schema == profiling_data::wire::SCHEMA)
                || (stream.source == freertos_profiling::SOURCE
                    && stream.report_type == freertos_profiling::REPORT_TYPE
                    && stream.schema == freertos_profiling::SCHEMA);
            let next = ProfilingSource {
                descriptor,
                status: None,
                age_ms: 0,
                assembly_losses: 0,
                supported,
            };
            if let Some(index) = position {
                let old = self.view.sources[index].descriptor.stream;
                if stream.epoch < old.epoch {
                    return;
                }
                if old == stream {
                    self.view.sources[index].descriptor = descriptor;
                    return;
                }
                self.assemblies[index].clear();
                self.view.sources[index] = next;
                if stream.source == profiling_data::wire::SOURCE {
                    self.audio = Default::default();
                }
                if stream.source == freertos_profiling::SOURCE {
                    self.view.tasks = Default::default();
                }
            } else if self.view.sources.len() < MAX_SOURCES {
                self.view.sources.push(next);
                self.assemblies.push(Assembly::default());
            }
            return;
        }
        let Some(index) = position else {
            return;
        };
        if self.view.sources[index].descriptor.stream != stream {
            return;
        }
        match record {
            Record::Status(status) => {
                if stream.source == freertos_profiling::SOURCE
                    && (!status.active
                        || self.view.sources[index]
                            .status
                            .is_none_or(|old| !old.active))
                {
                    self.view.tasks.pause();
                }
                self.view.sources[index].status = Some(status);
            }
            Record::Data(chunk) => {
                if !self.view.sources[index].supported {
                    return;
                }
                let source_limit = if stream.source == freertos_profiling::SOURCE {
                    65536
                } else {
                    MAX_REPORT_BYTES - 65536
                };
                if chunk.total as usize > source_limit {
                    self.view.sources[index].assembly_losses =
                        self.view.sources[index].assembly_losses.saturating_add(1);
                    return;
                }
                let retained: usize = self.assemblies.iter().map(Assembly::bytes).sum();
                if retained
                    .checked_add(chunk.payload.as_slice().len())
                    .is_none_or(|n| n > MAX_REPORT_BYTES)
                {
                    self.view.sources[index].assembly_losses += 1;
                    return;
                }
                let previous_lost = self.assemblies[index].lost;
                let result = self.assemblies[index].accept(&chunk);
                match result {
                    Ok(Some(bytes)) => {
                        let mut accepted = false;
                        if stream.source == profiling_data::wire::SOURCE {
                            if let Ok(view) = profiling_protocol::decode::<ProfilingView>(bytes) {
                                if view.valid() && view.generation >= self.audio.generation {
                                    self.audio = view;
                                    self.view.sources[index].age_ms = 0;
                                    accepted = true;
                                }
                            }
                        } else if stream.source == freertos_profiling::SOURCE {
                            if let Ok(sample) =
                                profiling_protocol::decode::<freertos_profiling::Snapshot>(bytes)
                            {
                                if sample.valid() {
                                    self.view.tasks.receive(sample);
                                    self.view.sources[index].age_ms = 0;
                                    accepted = true;
                                }
                            }
                        }
                        if !accepted {
                            self.view.sources[index].assembly_losses =
                                self.view.sources[index].assembly_losses.saturating_add(1);
                        }
                    }
                    Err(_) => {
                        self.view.sources[index].assembly_losses =
                            self.view.sources[index].assembly_losses.saturating_add(1)
                    }
                    _ => {}
                }
                self.view.sources[index].assembly_losses = self.view.sources[index]
                    .assembly_losses
                    .saturating_add(self.assemblies[index].lost.saturating_sub(previous_lost));
                self.assemblies[index].release();
            }
            _ => {}
        }
    }
}
