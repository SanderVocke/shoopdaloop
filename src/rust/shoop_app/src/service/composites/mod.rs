//! Service-owned authored documents and confirmed prepared versions.
use super::*;
use shoop_app_protocol::composite::{
    Document, Edit, Instance, PendingStage, TimingBasis, Versioned, View,
};
mod prepare;
pub(super) mod structure;

#[derive(Default)]
pub(super) struct Owner {
    pub active: Option<Versioned>,
    pub candidates: BTreeMap<u64, Versioned>,
    pub latest: Option<u64>,
    retained: Option<shoop_app_protocol::composite::Content>,
    revisions: BTreeMap<LoopId, u64>,
    candidate_revisions: BTreeMap<u64, BTreeMap<LoopId, u64>>,
    error: Option<String>,
}

impl Owner {
    pub(super) fn restored(active: Versioned, revisions: BTreeMap<LoopId, u64>) -> Self {
        Self {
            active: Some(active),
            revisions,
            ..Self::default()
        }
    }
    pub(super) fn has_retained(&self) -> bool {
        self.retained.is_some()
    }
    fn authored(&self) -> Option<&Versioned> {
        self.latest
            .and_then(|version| self.candidates.get(&version))
            .or(self.active.as_ref())
    }
}

impl Service {
    fn authored_loop_length(&self, id: LoopId) -> u64 {
        self.composites
            .get(&id)
            .and_then(Owner::authored)
            .map_or(self.loops[&id].view.length_frames, |plan| {
                plan.prepared.unit_frames * plan.prepared.iterations
            })
    }
    fn composite_geometry(&self, target: LoopId) -> (BTreeMap<u64, (u64, u64)>, Option<u64>) {
        let prepared = self.composites.get(&target).and_then(|owner| {
            owner
                .latest
                .and_then(|version| owner.candidates.get(&version))
                .map(|plan| &plan.prepared)
                .or_else(|| owner.retained.as_ref().map(|content| &content.prepared))
                .or_else(|| owner.active.as_ref().map(|plan| &plan.prepared))
        });
        prepared.map_or_else(
            || (BTreeMap::new(), None),
            |prepared| {
                (
                    prepared
                        .instances
                        .iter()
                        .map(|event| (event.instance_id, (event.start_frame, event.end_frame)))
                        .collect(),
                    Some(prepared.unit_frames),
                )
            },
        )
    }
    fn composite_format(&self) -> ConversionTarget {
        ConversionTarget {
            sample_rate: self.observed.status.sample_rate,
            quantum_frames: self.observed.status.quantum_frames,
        }
    }
    fn composite_basis(&self, target: LoopId) -> TimingBasis {
        let format = self.composite_format();
        let source = self
            .observed
            .loops
            .get(&self.loops[&target].backend)
            .and_then(|state| state.sync_source);
        let unit_frames = source
            .and_then(|source| {
                self.observed
                    .loops
                    .get(&source)
                    .map(|state| u64::from(state.length))
                    .or_else(|| {
                        self.observed
                            .composites
                            .get(&BackendCompositeId::from_raw(source.raw()))
                            .map(|state| state.length)
                    })
            })
            .filter(|length| *length > 0)
            .unwrap_or(u64::from(format.quantum_frames));
        TimingBasis {
            sample_rate: format.sample_rate,
            unit_frames,
        }
    }
    fn composite_document(
        &self,
        target: LoopId,
        allow_convert: bool,
    ) -> core::result::Result<Document, CommandError> {
        let entry = self.loop_entry(target)?;
        if entry.view.sync {
            return Err(CommandError::invalid(
                "the sync loop cannot become a composite",
            ));
        }
        if let Some(owner) = self.composites.get(&target) {
            if let Some(pending) = owner
                .latest
                .and_then(|version| owner.candidates.get(&version))
            {
                return Ok(pending.document.clone());
            }
            if let Some(retained) = &owner.retained {
                return Ok(retained.document.clone());
            }
            if let Some(active) = &owner.active {
                return Ok(active.document.clone());
            }
        }
        if !allow_convert
            || !entry.view.empty
            || entry.view.mode != LoopMode::Stopped
            || entry.view.structural_state != StructuralState::Confirmed
        {
            return Err(CommandError::invalid(
                "composition requires an existing composite or an empty stopped slot",
            ));
        }
        Ok(Document {
            kind: CompositeKind::Regular,
            basis: self.composite_basis(target),
            instances: Vec::new(),
        })
    }
    fn next_instance(&mut self) -> core::result::Result<u64, CommandError> {
        let id = self.next_composite_event;
        self.next_composite_event = id
            .checked_add(1)
            .ok_or_else(|| CommandError::invalid("composite event identity space exhausted"))?;
        Ok(id)
    }
    pub(super) fn edit_composite(
        &mut self,
        edit: Edit,
    ) -> core::result::Result<Effect, CommandError> {
        if self.backend.composite_limits().is_none() {
            return Err(CommandError::unsupported(BackendCapability::CompositeLoops));
        }
        if !edit.bounded() {
            return Err(CommandError::invalid("invalid composite edit"));
        }
        let target = edit.target();
        self.loop_entry(target)?;
        if let Edit::Cancel { version, .. } = edit {
            let id = BackendCompositeId::from_raw(self.loops[&target].backend.raw());
            self.backend.cancel_composite_plan(id, version)?;
            return Ok(Effect::None);
        }
        let allow_convert = matches!(
            edit,
            Edit::Convert { .. }
                | Edit::Append {
                    convert_if_empty: true,
                    ..
                }
        );
        let mut document = self.composite_document(target, allow_convert)?;
        let (mut geometry, mut unit) = self.composite_geometry(target);
        match edit {
            Edit::Convert { .. } => {
                if self.composites.contains_key(&target) {
                    return Err(CommandError::invalid("loop is already a composite"));
                }
            }
            Edit::Append { add, parallel, .. } => {
                for id in &add {
                    self.loop_entry(*id)?;
                }
                if add.is_empty() {
                    return Ok(Effect::None);
                }
                document.kind = CompositeKind::Regular;
                for event in &mut document.instances {
                    event.mode = None;
                }
                let prepared = prepare::compile_with_geometry(
                    target,
                    &document,
                    self.composite_format(),
                    self.backend.composite_limits().unwrap(),
                    |id| Ok(self.authored_loop_length(id)),
                    &geometry,
                    unit,
                )?;
                let start_frame = if parallel {
                    prepared
                        .instances
                        .iter()
                        .map(|event| event.start_frame)
                        .max()
                        .unwrap_or(0)
                } else {
                    prepared
                        .iterations
                        .checked_mul(prepared.unit_frames)
                        .ok_or_else(|| anyhow!("composite duration overflow"))?
                };
                let mut start = if start_frame == 0 {
                    0
                } else {
                    prepare::cycles_for_frames(document.basis, start_frame, prepared.sample_rate)?
                };
                for source in add {
                    let duration = prepare::cycles_for_frames(
                        document.basis,
                        self.authored_loop_length(source),
                        prepared.sample_rate,
                    )?;
                    document.instances.push(Instance {
                        instance_id: self.next_instance()?,
                        loop_id: source,
                        start_cycle: start,
                        n_cycles: None,
                        mode: None,
                    });
                    if !parallel {
                        start = start
                            .checked_add(duration)
                            .ok_or_else(|| anyhow!("composite position overflow"))?;
                    }
                }
            }
            Edit::Insert {
                source,
                start_cycle,
                ..
            } => {
                self.loop_entry(source)?;
                document.instances.push(Instance {
                    instance_id: self.next_instance()?,
                    loop_id: source,
                    start_cycle,
                    n_cycles: None,
                    mode: (document.kind == CompositeKind::Script).then_some(LoopMode::Playing),
                });
            }
            Edit::Delete { events, .. }
            | Edit::Relocate {
                events,
                duplicate: false,
                ..
            } if events.is_empty() => return Ok(Effect::None),
            Edit::Delete { events, .. } => {
                let selected = selected_events(&document, &events)?;
                document
                    .instances
                    .retain(|event| !selected.contains(&event.instance_id));
            }
            Edit::Relocate {
                events,
                start_cycle,
                duplicate,
                ..
            } => {
                let selected = selected_events(&document, &events)?;
                let origin = document
                    .instances
                    .iter()
                    .filter(|event| selected.contains(&event.instance_id))
                    .map(|event| event.start_cycle)
                    .min()
                    .unwrap_or(start_cycle);
                let moved = document
                    .instances
                    .iter()
                    .filter(|event| selected.contains(&event.instance_id))
                    .cloned()
                    .collect::<Vec<_>>();
                if !duplicate {
                    document
                        .instances
                        .retain(|event| !selected.contains(&event.instance_id));
                }
                for mut event in moved {
                    let previous_id = event.instance_id;
                    let previous_cycle = event.start_cycle;
                    event.start_cycle = start_cycle
                        .checked_add(event.start_cycle - origin)
                        .ok_or_else(|| anyhow!("composite position overflow"))?;
                    if duplicate {
                        event.instance_id = self.next_instance()?;
                    }
                    if let Some(&(start, end)) = geometry.get(&previous_id) {
                        let start_frame = if event.start_cycle == previous_cycle {
                            start
                        } else {
                            prepare::frame_at(
                                document.basis,
                                event.start_cycle,
                                self.composite_format(),
                            )?
                        };
                        let end_frame = start_frame
                            .checked_add(end - start)
                            .ok_or_else(|| anyhow!("composite duration overflow"))?;
                        geometry.insert(event.instance_id, (start_frame, end_frame));
                    }
                    document.instances.push(event);
                }
            }
            Edit::Duration {
                event, n_cycles, ..
            } => {
                let instance = document
                    .instances
                    .iter_mut()
                    .find(|instance| instance.instance_id == event)
                    .ok_or_else(|| CommandError::invalid("stale composite event"))?;
                if instance.n_cycles != n_cycles {
                    geometry.remove(&event);
                }
                instance.n_cycles = n_cycles;
            }
            Edit::Kind { kind, .. } => {
                document.kind = kind;
                for event in &mut document.instances {
                    event.mode = (kind == CompositeKind::Script).then_some(LoopMode::Playing);
                }
            }
            Edit::Mode { event, mode, .. } => {
                if document.kind != CompositeKind::Script {
                    return Err(CommandError::invalid(
                        "regular composite events inherit Play",
                    ));
                }
                document
                    .instances
                    .iter_mut()
                    .find(|instance| instance.instance_id == event)
                    .ok_or_else(|| CommandError::invalid("stale composite event"))?
                    .mode = Some(mode);
            }
            Edit::Timing { basis, .. } => {
                document.basis = basis;
                geometry.clear();
                unit = None;
            }
            Edit::Cancel { .. } => unreachable!(),
        }
        self.submit_composite_geometry(target, document, &geometry, unit)?;
        Ok(Effect::CompositeEdit(target))
    }

    fn submit_composite_document(
        &mut self,
        target: LoopId,
        document: Document,
    ) -> core::result::Result<(), CommandError> {
        let (geometry, unit) = self.composite_geometry(target);
        self.submit_composite_geometry(target, document, &geometry, unit)
    }

    fn submit_composite_geometry(
        &mut self,
        target: LoopId,
        document: Document,
        geometry: &BTreeMap<u64, (u64, u64)>,
        unit: Option<u64>,
    ) -> core::result::Result<(), CommandError> {
        for event in &document.instances {
            self.loop_entry(event.loop_id)?;
            match event.mode {
                Some(LoopMode::Recording) if self.composites.contains_key(&event.loop_id) => {
                    return Err(CommandError::invalid(
                        "a composite event targeting another composite must use Play or Stop",
                    ));
                }
                Some(LoopMode::Replacing) => {
                    return Err(CommandError::unsupported(BackendCapability::ReplacingMode))
                }
                Some(LoopMode::PlayingDryThroughWet | LoopMode::RecordingDryIntoWet) => {
                    return Err(CommandError::unsupported(BackendCapability::DryWetModes))
                }
                _ => {}
            }
        }
        self.validate_composite_dependencies(target, &document)?;
        let prepared = prepare::compile_with_geometry(
            target,
            &document,
            self.composite_format(),
            self.backend
                .composite_limits()
                .ok_or_else(|| anyhow!("prepared composites unavailable"))?,
            |id| Ok(self.authored_loop_length(id)),
            geometry,
            unit,
        )
        .map_err(|error| CommandError::invalid(error.to_string()))?;
        self.submit_prepared_composite(target, document, prepared)
    }

    fn submit_prepared_composite(
        &mut self,
        target: LoopId,
        document: Document,
        prepared: shoop_app_protocol::composite::Prepared,
    ) -> core::result::Result<(), CommandError> {
        let backend = self.loops[&target].backend;
        let sync_source = self
            .observed
            .composites
            .get(&BackendCompositeId::from_raw(backend.raw()))
            .map(|state| state.sync_source)
            .or_else(|| {
                self.observed
                    .loops
                    .get(&backend)
                    .map(|state| state.sync_source)
            })
            .flatten();
        let spans = prepared
            .spans
            .iter()
            .map(|span| BackendCompositeSpan {
                target: if self.composites.contains_key(&span.target) {
                    BackendCompositeTarget::Composite(BackendCompositeId::from_raw(
                        self.loops[&span.target].backend.raw(),
                    ))
                } else {
                    BackendCompositeTarget::Loop(self.loops[&span.target].backend)
                },
                start: span.start,
                end: span.end,
                occurrence_start: span.occurrence_start,
                ordinal: span.ordinal,
                mode: match span.mode {
                    LoopMode::Stopped => BackendLoopMode::Stopped,
                    LoopMode::Playing => BackendLoopMode::Playing,
                    LoopMode::Recording => BackendLoopMode::Recording,
                    _ => unreachable!("compiler validates modes"),
                },
            })
            .collect();
        let plan = BackendPreparedCompositePlan {
            kind: if prepared.spans.is_empty() || document.kind == CompositeKind::Regular {
                BackendCompositeKind::Regular
            } else {
                BackendCompositeKind::Script
            },
            sync_source,
            unit_frames: prepared.unit_frames,
            iterations: prepared.iterations,
            entries: document.instances.len() as u32,
            spans,
        };
        let version = if self
            .composites
            .get(&target)
            .is_some_and(|owner| owner.active.is_some() || !owner.candidates.is_empty())
        {
            self.backend.configure_prepared_composite_loop(
                BackendCompositeId::from_raw(backend.raw()),
                &plan,
            )?
        } else {
            self.backend
                .convert_empty_loop_to_prepared_composite(backend, &plan)?
                .1
        };
        let revisions = document
            .instances
            .iter()
            .map(|event| {
                (
                    event.loop_id,
                    self.loops[&event.loop_id].view.content_revision,
                )
            })
            .collect();
        let owner = self.composites.entry(target).or_default();
        owner.candidates.insert(
            version,
            Versioned {
                version,
                document,
                prepared,
            },
        );
        owner.latest = Some(version);
        owner.candidate_revisions.insert(version, revisions);
        owner.error = None;
        self.bump();
        Ok(())
    }

    fn validate_composite_dependencies(
        &self,
        target: LoopId,
        document: &Document,
    ) -> core::result::Result<(), CommandError> {
        let mut graph: BTreeMap<LoopId, BTreeSet<LoopId>> = BTreeMap::new();
        for (&parent, owner) in &self.composites {
            if parent == target {
                continue;
            }
            for plan in owner.active.iter().chain(owner.candidates.values()) {
                for event in &plan.document.instances {
                    graph.entry(parent).or_default().insert(event.loop_id);
                }
            }
        }
        for event in &document.instances {
            graph.entry(target).or_default().insert(event.loop_id);
        }
        fn visit(
            id: LoopId,
            graph: &BTreeMap<LoopId, BTreeSet<LoopId>>,
            visiting: &mut BTreeSet<LoopId>,
            done: &mut BTreeSet<LoopId>,
        ) -> bool {
            if done.contains(&id) {
                return true;
            }
            if !visiting.insert(id) {
                return false;
            }
            if graph.get(&id).is_some_and(|children| {
                children
                    .iter()
                    .any(|child| !visit(*child, graph, visiting, done))
            }) {
                return false;
            }
            visiting.remove(&id);
            done.insert(id);
            true
        }
        if graph
            .keys()
            .any(|id| !visit(*id, &graph, &mut BTreeSet::new(), &mut BTreeSet::new()))
        {
            return Err(CommandError::invalid("composite dependency cycle"));
        }
        Ok(())
    }

    pub(super) fn observe_composites(&mut self, observed: &BackendSnapshot) {
        let structural = self.composite_structure.is_some();
        let mut changed = false;
        for receipt in &observed.composite_receipts {
            let Some((&id, _)) = self
                .loops
                .iter()
                .find(|(_, entry)| entry.backend.raw() == receipt.composite.raw())
            else {
                continue;
            };
            let Some(owner) = self.composites.get_mut(&id) else {
                continue;
            };
            if let Some(candidate) = owner.candidates.remove(&receipt.version) {
                changed = true;
                let revisions = owner
                    .candidate_revisions
                    .remove(&receipt.version)
                    .unwrap_or_default();
                if matches!(receipt.outcome, BackendCompositeOutcome::Applied { .. }) {
                    if !structural
                        || owner.retained.as_ref().is_some_and(|retained| {
                            retained.document == candidate.document
                                && retained.prepared == candidate.prepared
                        })
                    {
                        owner.retained = None;
                    }
                    owner.active = Some(candidate);
                    owner.revisions = revisions;
                    owner.error = None;
                } else if let BackendCompositeOutcome::Refused(message) = &receipt.outcome {
                    owner.error = Some(message.clone());
                }
            }
            if owner.latest == Some(receipt.version) {
                owner.latest = owner.candidates.keys().next_back().copied();
            }
        }
        for removed in &observed.removed_composites {
            if let Some((&id, _)) = self
                .loops
                .iter()
                .find(|(_, entry)| entry.backend.raw() == removed.raw())
            {
                changed |= self.composites.remove(&id).is_some();
            }
        }
        if changed {
            self.bump();
        }
        self.composites.retain(|id, owner| {
            self.loops.contains_key(id) && (owner.active.is_some() || !owner.candidates.is_empty())
        });
    }

    pub(super) fn composite_transport_iteration(
        &self,
        id: LoopId,
        align: Option<u32>,
    ) -> core::result::Result<Option<i64>, CommandError> {
        let Some(cycle) = align else {
            return Ok(None);
        };
        let active = self.composites[&id]
            .active
            .as_ref()
            .or_else(|| self.composites[&id].authored())
            .ok_or_else(|| anyhow!("composite has no accepted plan"))?;
        let frame = prepare::frame_at(
            active.document.basis,
            u64::from(cycle),
            ConversionTarget {
                sample_rate: active.prepared.sample_rate,
                quantum_frames: active.prepared.quantum,
            },
        )?;
        let iteration = frame / active.prepared.unit_frames;
        if iteration >= active.prepared.iterations
            && !(iteration == 0 && active.prepared.iterations == 0)
        {
            return Err(CommandError::invalid(
                "composite iteration is outside the accepted arrangement",
            ));
        }
        Ok(Some(
            i64::try_from(iteration).map_err(|_| anyhow!("composite iteration overflow"))?,
        ))
    }

    pub(super) fn require_restored_composite(
        &self,
        id: LoopId,
        mode: BackendLoopMode,
    ) -> Result<(), CommandError> {
        if mode != BackendLoopMode::Stopped
            && self
                .composites
                .get(&id)
                .is_some_and(|owner| owner.retained.is_some())
        {
            return Err(CommandError {
                reason: Refusal::Busy,
                message: "restore the retained arrangement with an editor change before playing it"
                    .into(),
            });
        }
        Ok(())
    }
    pub(super) fn transition_authored_composite(
        &mut self,
        id: LoopId,
        mode: BackendLoopMode,
        delay: Option<u32>,
        align: Option<u32>,
    ) -> core::result::Result<(), CommandError> {
        let backend = self.loops[&id].backend;
        if mode == BackendLoopMode::Stopped {
            self.backend.transition_loop(backend, mode, delay)?;
            return Ok(());
        }
        let iteration = self.composite_transport_iteration(id, align)?;
        self.backend.transition_composite_loop(
            BackendCompositeId::from_raw(backend.raw()),
            mode,
            if iteration.is_some() { None } else { delay },
            iteration,
        )?;
        Ok(())
    }

    pub(super) fn refresh_natural_composites(&mut self) {
        if self.composite_structure.is_some() {
            return;
        }
        let refresh = self
            .composites
            .iter()
            .filter_map(|(&id, owner)| {
                let active = owner.active.as_ref()?;
                if !owner.candidates.is_empty() || owner.retained.is_some() {
                    return None;
                }
                active
                    .document
                    .instances
                    .iter()
                    .any(|event| {
                        event.n_cycles.is_none()
                            && self.loops.get(&event.loop_id).is_some_and(|entry| {
                                entry.view.mode != LoopMode::Recording
                                    && entry.view.next_mode != LoopMode::Recording
                                    && owner.revisions.get(&event.loop_id)
                                        != Some(&entry.view.content_revision)
                            })
                    })
                    .then(|| (id, active.document.clone()))
            })
            .collect::<Vec<_>>();
        for (target, document) in refresh {
            let (mut geometry, unit) = self.composite_geometry(target);
            for event in &document.instances {
                if event.n_cycles.is_none()
                    && self.composites[&target].revisions.get(&event.loop_id)
                        != Some(&self.loops[&event.loop_id].view.content_revision)
                {
                    geometry.remove(&event.instance_id);
                }
            }
            let prepared = self
                .backend
                .composite_limits()
                .ok_or_else(|| CommandError::unsupported(BackendCapability::CompositeLoops))
                .and_then(|limits| {
                    prepare::compile_with_geometry(
                        target,
                        &document,
                        self.composite_format(),
                        limits,
                        |id| Ok(self.loops[&id].view.length_frames),
                        &geometry,
                        unit,
                    )
                    .map_err(|error| CommandError::invalid(error.to_string()))
                });
            if prepared.as_ref().ok()
                == self.composites[&target]
                    .active
                    .as_ref()
                    .map(|active| &active.prepared)
            {
                let revisions = document
                    .instances
                    .iter()
                    .map(|event| {
                        (
                            event.loop_id,
                            self.loops[&event.loop_id].view.content_revision,
                        )
                    })
                    .collect();
                self.composites.get_mut(&target).unwrap().revisions = revisions;
                continue;
            }
            if let Err(error) = prepared
                .and_then(|prepared| self.submit_prepared_composite(target, document, prepared))
            {
                if let Some(owner) = self.composites.get_mut(&target) {
                    owner.error = Some(error.message);
                    if let Some(active) = &owner.active {
                        owner.revisions = active
                            .document
                            .instances
                            .iter()
                            .filter_map(|event| {
                                self.loops
                                    .get(&event.loop_id)
                                    .map(|entry| (event.loop_id, entry.view.content_revision))
                            })
                            .collect();
                    }
                }
            }
        }
    }

    pub(super) fn composite_views(&self) -> Vec<View> {
        self.composites
            .iter()
            .map(|(&loop_id, owner)| {
                let state = self.observed.composites.get(&BackendCompositeId::from_raw(
                    self.loops[&loop_id].backend.raw(),
                ));
                View {
                    mode: state.map_or(LoopMode::Unknown, |state| {
                        super::projection::mode(state.mode)
                    }),
                    position_frames: state.map_or(0, |state| state.position),
                    length_frames: state.map_or(0, |state| state.length),
                    iteration: state
                        .and_then(|state| {
                            owner.active.as_ref().map(|active| {
                                state.position.min(state.length.saturating_sub(1))
                                    / active.prepared.unit_frames
                            })
                        })
                        .unwrap_or(0),
                    passes: state.map_or(0, |state| state.cycle_count),
                    next_mode: state
                        .and_then(|state| state.next_mode)
                        .map(super::projection::mode),
                    next_transition_delay: state.and_then(|state| state.next_transition_delay),
                    observed_frame: state.map(|state| state.observed_frame),
                    repeat_sync: state.map_or(self.loops[&loop_id].view.repeat_sync, |state| {
                        state.repeat_sync
                    }),
                    working: self
                        .composite_structure
                        .as_ref()
                        .and_then(|job| job.working(loop_id))
                        .or_else(|| owner.retained.clone()),
                    error: owner.error.clone(),
                    loop_id,
                    active: owner.active.clone(),
                    pending: owner
                        .latest
                        .and_then(|version| owner.candidates.get(&version))
                        .cloned(),
                    pending_stage: owner
                        .latest
                        .and_then(|version| {
                            self.observed.composite_operations.iter().find(|operation| {
                                operation.composite.raw() == self.loops[&loop_id].backend.raw()
                                    && operation.version == version
                            })
                        })
                        .map(|operation| match operation.stage {
                            BackendCompositePending::Queued => PendingStage::Queued,
                            BackendCompositePending::Staging => PendingStage::Staging,
                            BackendCompositePending::AwaitingRestart => {
                                PendingStage::AwaitingRestart
                            }
                            BackendCompositePending::Confirming => PendingStage::Confirming,
                            BackendCompositePending::Cancelling => PendingStage::Cancelling,
                        })
                        .or_else(|| owner.latest.map(|_| PendingStage::Queued)),
                    waiting_repeat: state.is_some_and(|state| state.waiting_repeat),
                    active_applied_frame: state.and_then(|state| state.active_applied_frame),
                }
            })
            .collect()
    }
}

fn selected_events(
    document: &Document,
    events: &[u64],
) -> core::result::Result<BTreeSet<u64>, CommandError> {
    let selected = events.iter().copied().collect::<BTreeSet<_>>();
    if selected.len() != events.len()
        || selected.iter().any(|id| {
            !document
                .instances
                .iter()
                .any(|event| event.instance_id == *id)
        })
    {
        return Err(CommandError::invalid("stale or duplicate composite event"));
    }
    Ok(selected)
}
