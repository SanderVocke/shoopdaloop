use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContentRetime {
    pub target: LoopId,
    pub expected_revision: Revision,
    pub expected_length: u64,
    pub revision: Revision,
    pub length: u64,
    pub restart: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

    fn id(slot: u32) -> LoopId {
        LoopId::from_parts(slot, 1).unwrap()
    }
    fn resolver() -> Resolver {
        let topology = compile(
            Limits::default(),
            &TopologyDescription {
                loops: [(0, 96, None), (1, 384, Some(id(0))), (2, 64, None)]
                    .into_iter()
                    .map(|(slot, length, sync_source)| LoopSpec {
                        id: id(slot),
                        kind: LoopKind::Primitive,
                        length,
                        sync_source,
                    })
                    .collect(),
                ..Default::default()
            },
        )
        .unwrap();
        let resolver = Resolver::prepare(topology, rt_runtime::Quantum::new(32).unwrap()).unwrap();

        resolver
    }
    fn tick(resolver: &mut Resolver) {
        resolver.preview_tick(&[]).unwrap();
        resolver.commit().unwrap();
        resolver.finish().unwrap();
    }
    fn change(target: u32, previous: u64, length: u64) -> ContentRetime {
        ContentRetime {
            target: id(target),
            expected_revision: Revision(0),
            expected_length: previous,
            revision: Revision(1),
            length,
            restart: false,
        }
    }

    #[shoop_test]
    fn family_retime_preserves_wraps_cycles_pending_counts_and_unrelated_playback() {
        let mut resolver = resolver();
        for slot in 0..3 {
            resolver
                .submit(Request {
                    target: id(slot),
                    sequence: Sequence(u64::from(slot)),
                    at: Frame(0),
                    timing: Timing::At,
                    transition: Transition::Play {
                        position: 0,
                        retrigger: false,
                    },
                })
                .unwrap();
        }
        tick(&mut resolver);
        let delayed = resolver
            .submit(Request {
                target: id(1),
                sequence: Sequence(10),
                at: Frame(32),
                timing: Timing::AfterTicks {
                    source: id(0),
                    skip: 0,
                },
                transition: Transition::Stop,
            })
            .unwrap();
        tick(&mut resolver);
        tick(&mut resolver);
        let unrelated = resolver.state(id(2)).unwrap();
        assert_eq!(resolver.state(id(0)).unwrap().position, 96);
        let changes = [change(0, 96, 128), change(1, 384, 512)];
        assert_no_heap(|| resolver.retime_content(changes.into_iter())).unwrap();
        assert_eq!(resolver.state(id(0)).unwrap().position, 128);
        assert_eq!(resolver.state(id(1)).unwrap().position, 128);
        assert_eq!(resolver.state(id(2)).unwrap(), unrelated);
        resolver.preview_tick(&[]).unwrap();
        let source = resolver
            .decisions()
            .unwrap()
            .iter()
            .find(|decision| decision.state.id == id(0))
            .unwrap();
        assert_eq!(source.state.position, 0);
        assert_eq!(source.state.cycles, 1);
        assert!(source.playback_discontinuity);
        resolver.commit().unwrap();
        assert_eq!(resolver.state(id(1)).unwrap().mode, Mode::Stopped);
        assert_eq!(
            resolver.completion(delayed).unwrap().unwrap().outcome,
            Outcome::Applied { frame: Frame(96) }
        );
        assert_eq!(resolver.state(id(0)).unwrap().last_pulse, Some(Frame(96)));
        assert!(resolver
            .events()
            .iter()
            .any(|event| event.target == id(0) && event.kind == EventKind::Wrap));
        resolver.finish().unwrap();
        assert!(!resolver.retimed_pending);
    }

    #[shoop_test]
    fn stale_or_recording_member_refuses_every_change_before_publication() {
        let mut resolver = resolver();
        let before = resolver.states.clone();
        for changes in [
            [change(0, 96, 128), change(1, 383, 512)],
            [change(0, 96, 128), change(0, 96, 128)],
            [change(0, 96, 128), change(1, 384, 511)],
        ] {
            assert!(assert_no_heap(|| resolver.retime_content(changes.into_iter())).is_err());
            assert_eq!(resolver.states, before);
        }
        resolver
            .submit(Request {
                target: id(1),
                sequence: Sequence(1),
                at: Frame(0),
                timing: Timing::At,
                transition: Transition::Record {
                    monitor_previous: false,
                },
            })
            .unwrap();
        tick(&mut resolver);
        let before = resolver.states.clone();
        assert!(assert_no_heap(
            || resolver.retime_content([change(0, 96, 128), change(1, 384, 512)].into_iter())
        )
        .is_err());
        assert_eq!(resolver.states, before);
    }
}

fn position(position: u64, previous: u64, length: u64, quantum: u64) -> Result<u64, Error> {
    if previous == 0 || length == 0 {
        return Ok(0);
    }
    let scaled = u64::try_from(u128::from(position) * u128::from(length) / u128::from(previous))
        .map_err(|_| Error::Exhausted)?;
    Ok(scaled / quantum * quantum)
}

impl Resolver {
    /// Replace primitive periods as one cut, preserving clocks and pending cycle counts.
    pub fn retime_content(
        &mut self,
        changes: impl Iterator<Item = ContentRetime> + Clone,
    ) -> Result<(), Error> {
        if self
            .topology
            .nodes()
            .iter()
            .any(|node| node.kind != LoopKind::Primitive)
        {
            return Err(Error::InvalidState);
        }
        self.update_content(changes)
    }

    /// Publish a complete same-length primitive revision without retiming its
    /// program or sync dependents.
    pub fn replace_content(&mut self, change: ContentRetime) -> Result<(), Error> {
        if change.length != change.expected_length
            || self.topology.node(change.target)?.kind != LoopKind::Primitive
        {
            return Err(Error::InvalidState);
        }
        self.update_content(core::iter::once(change))
    }

    fn update_content(
        &mut self,
        changes: impl Iterator<Item = ContentRetime> + Clone,
    ) -> Result<(), Error> {
        let quantum = self.quantum.frames() as u64;
        self.scratch.states.copy_from_slice(&self.states);
        for (index, change) in changes.clone().enumerate() {
            if changes
                .clone()
                .take(index)
                .any(|previous| previous.target == change.target)
            {
                return Err(Error::DuplicateId);
            }
            let state = self.state(change.target)?;
            if state.mode == Mode::Recording
                || state.content_length != change.expected_length
                || state.content_revision != change.expected_revision
                || change.revision <= state.content_revision
            {
                return Err(Error::InvalidState);
            }
            if change.length % quantum != 0 {
                return Err(Error::InvalidFrame);
            }
            let staged = self.scratch.states[change.target.slot() as usize]
                .as_mut()
                .ok_or(Error::InvalidId)?;
            staged.position = if change.restart {
                0
            } else {
                position(state.position, state.content_length, change.length, quantum)?
            };
            staged.content_length = change.length;
            staged.content_revision = change.revision;
            staged.revision_watermark = staged.revision_watermark.max(change.revision);
            if change.length == 0 {
                staged.mode = Mode::Stopped;
            }
        }
        self.scratch.records.copy_from_slice(&self.records);
        for record in self.scratch.records.iter_mut().flatten() {
            if record.terminal.is_some() {
                continue;
            }
            let Some(change) = changes
                .clone()
                .find(|change| change.target == record.request.target)
            else {
                continue;
            };
            match &mut record.request.transition {
                Transition::Play {
                    position: requested,
                    ..
                }
                | Transition::Seek {
                    position: requested,
                    ..
                } => {
                    *requested =
                        position(*requested, change.expected_length, change.length, quantum)?;
                }
                _ => {}
            }
        }
        self.states.copy_from_slice(&self.scratch.states);
        self.records.copy_from_slice(&self.scratch.records);
        for change in changes {
            self.retimed_pending = true;
            self.retimed[change.target.slot() as usize] = Some(Transition::Replace {
                length: change.length,
                revision: change.revision,
                preserve_position: !change.restart,
            });
        }
        self.preview = None;
        Ok(())
    }
}
