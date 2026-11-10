use super::*;

impl Resolver {
    pub(crate) fn invalidate_compact(&mut self) {
        self.compact_ready = false;
    }

    pub(crate) fn compact_tick(
        &mut self,
        observations: &[Observation],
    ) -> Result<Option<Interval>, Error> {
        if !observations.is_empty()
            || self.quiescent
            || self.committed
            || self.program_change.is_some()
            || self.retimed_pending
        {
            return Ok(None);
        }
        if !self.compact_ready {
            if !self.topology.triggers().is_empty()
                || self.records.iter().flatten().any(|r| r.terminal.is_none())
                || self.topology.nodes().iter().any(|node| {
                    node.kind != LoopKind::Primitive
                        || self.states[node.id.slot() as usize].is_none_or(|s| {
                            s.composite.is_some()
                                || !matches!(
                                    s.mode,
                                    Mode::Stopped | Mode::Playing | Mode::Recording
                                )
                        })
                })
            {
                return Ok(None);
            }
            self.compact_ready = true;
        }
        let related = self
            .topology
            .nodes()
            .iter()
            .any(|n| n.sync_source.is_some());
        if related
            && self.topology.nodes().iter().any(|n| {
                let state = self.states[n.id.slot() as usize].expect("active state");
                state.content_length != 0
                    && state.position == state.content_length
                    && (state.mode.playback_mode()
                        || state.mode == Mode::Recording && state.monitor_previous)
            })
        {
            return Ok(None);
        }
        let end = self
            .frame
            .checked_add(self.quantum.frames())
            .map_err(|_| Error::Exhausted)?;
        let q = self.quantum.frames() as u64;
        self.scratch.decisions.clear();
        self.scratch.events.clear();
        self.scratch.dropped = 0;
        for node in self.topology.nodes() {
            let slot = node.id.slot() as usize;
            let mut state = self.states[slot].ok_or(Error::InvalidState)?;
            let wrap = state.content_length != 0
                && state.position == state.content_length
                && (state.mode.playback_mode()
                    || state.mode == Mode::Recording && state.monitor_previous);
            if wrap {
                state.position = 0;
                state.cycles = state.cycles.checked_add(1).ok_or(Error::Exhausted)?;
                state.last_pulse = Some(self.frame);
                for kind in [EventKind::Wrap, EventKind::Pulse] {
                    if self.scratch.events.len() < self.topology.limits().journal_events {
                        self.scratch.events.push(Event {
                            frame: self.frame,
                            target: node.id,
                            kind,
                        });
                    } else {
                        self.scratch.dropped = self.scratch.dropped.saturating_add(1);
                    }
                }
            }
            if state.content_length % q != 0
                || state.position % q != 0
                || state.capture_frames % q != 0
                || state.advances()
                    && state
                        .position
                        .checked_add(q)
                        .is_none_or(|p| p > state.content_length)
            {
                return Err(Error::InvalidFrame);
            }
            state
                .capture_frames
                .checked_add(if state.mode.records() { q } else { 0 })
                .ok_or(Error::Exhausted)?;
            self.scratch.decisions.push(Decision {
                state,
                playback_discontinuity: wrap && state.plays(),
                origin: None,
            });
        }
        let interval = Interval {
            start: self.frame,
            end,
        };
        self.preview = Some(interval);
        self.preview_program_epoch = None;
        self.compact_preview = true;
        Ok(Some(interval))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

    #[shoop_test]
    fn compact_ticks_match_transactional_clock_states_events_and_pending_transitions() {
        for q in [1, 32] {
            for sync in [false, true] {
                let id = |i| LoopId::from_parts(i, 1).unwrap();
                let description = TopologyDescription {
                    loops: (0..3)
                        .map(|i| LoopSpec {
                            id: id(i),
                            kind: LoopKind::Primitive,
                            length: if i == 2 {
                                0
                            } else {
                                (6 + 4 * i as usize) as u64 * q as u64
                            },
                            sync_source: (sync && i == 1).then(|| id(0)),
                        })
                        .collect(),
                    ..TopologyDescription::default()
                };
                let make = || {
                    Resolver::prepare(
                        compile(Limits::default(), &description).unwrap(),
                        rt_runtime::Quantum::new(q).unwrap(),
                    )
                    .unwrap()
                };
                let mut reference = make();
                let mut compact = make();
                let mut pending = Vec::with_capacity(32);
                let mut fast = 0;
                assert_no_heap(|| {
                    for tick in 0..256 {
                        for i in 0..3 {
                            let transition = if tick == 0 {
                                Some(if i == 2 {
                                    Transition::Record {
                                        monitor_previous: false,
                                    }
                                } else {
                                    Transition::Play {
                                        position: 0,
                                        retrigger: false,
                                    }
                                })
                            } else if tick == 80 && i == 1 {
                                Some(Transition::Stop)
                            } else if tick == 88 && i == 0 {
                                Some(Transition::Seek {
                                    position: 0,
                                    retrigger: true,
                                })
                            } else if tick == 100 && i == 1 {
                                Some(Transition::Play {
                                    position: 0,
                                    retrigger: false,
                                })
                            } else if tick == 128 && i == 0 {
                                Some(Transition::Record {
                                    monitor_previous: true,
                                })
                            } else if tick == 160 && i == 2 {
                                Some(Transition::Stop)
                            } else {
                                None
                            };
                            if let Some(transition) = transition {
                                let request = Request {
                                    target: id(i),
                                    sequence: Sequence(tick * 3 + i as u64),
                                    at: reference.frame(),
                                    timing: if tick == 100 && sync {
                                        Timing::AfterTicks {
                                            source: id(0),
                                            skip: 1,
                                        }
                                    } else {
                                        Timing::At
                                    },
                                    transition,
                                };
                                pending.push((
                                    reference.submit(request).unwrap(),
                                    compact.submit(request).unwrap(),
                                ));
                            }
                        }
                        reference.preview_tick(&[]).unwrap();
                        if compact.compact_tick(&[]).unwrap().is_some() {
                            fast += 1;
                        } else {
                            compact.preview_tick(&[]).unwrap();
                        }
                        assert_eq!(
                            reference.decisions().unwrap(),
                            compact.decisions().unwrap(),
                            "q={q}, sync={sync}, tick={tick}"
                        );
                        reference.commit().unwrap();
                        compact.commit().unwrap();
                        assert_eq!(reference.events(), compact.events());
                        assert_eq!(reference.dropped_events(), compact.dropped_events());
                        reference.finish().unwrap();
                        compact.finish().unwrap();
                        let mut index = 0;
                        while index < pending.len() {
                            let (a, b) = pending[index];
                            assert_eq!(
                                reference.completion(a).unwrap(),
                                compact.completion(b).unwrap()
                            );
                            if reference.completion(a).unwrap().is_some() {
                                reference.take_completion(a).unwrap();
                                compact.take_completion(b).unwrap();
                                pending.swap_remove(index);
                            } else {
                                index += 1;
                            }
                        }
                        for i in 0..3 {
                            assert_eq!(reference.state(id(i)), compact.state(id(i)));
                        }
                    }
                    assert!(fast > 100, "fast ticks={fast}");
                });
            }
        }
    }
}
