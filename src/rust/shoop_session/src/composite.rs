use crate::*;
use std::collections::{BTreeMap, BTreeSet};

fn invalid(message: &str) -> SessionError {
    SessionError::Validation(message.to_owned())
}

/// Resolves absent legacy timing entirely from the saved sample clock and loop lengths.
pub fn materialize_composite_timing(document: &mut SessionDocument) -> Result<(), SessionError> {
    let loops = document
        .track_groups
        .iter()
        .flat_map(|g| &g.tracks)
        .flat_map(|t| &t.loops)
        .map(|l| (l.id, l.length_frames))
        .collect::<BTreeMap<_, _>>();
    let source = document
        .track_groups
        .iter()
        .flat_map(|g| &g.tracks)
        .filter(|t| t.is_sync)
        .flat_map(|t| &t.loops)
        .find(|l| l.is_sync)
        .map(|l| l.id);
    let source_unit = source
        .and_then(|id| loops.get(&id).copied())
        .filter(|length| *length > 0)
        .unwrap_or(1);
    for loop_ in document
        .track_groups
        .iter_mut()
        .flat_map(|g| &mut g.tracks)
        .flat_map(|t| &mut t.loops)
    {
        if loop_.execution.is_none() {
            loop_.execution = Some(LoopExecutionDocument {
                sync_source: if document.global.sync && !loop_.is_sync {
                    source
                } else {
                    None
                },
                repeat_sync: document.global.sync && !loop_.is_sync,
            });
        }
        let Some(composite) = &mut loop_.composite else {
            continue;
        };
        if composite.timing.is_some() {
            continue;
        }
        if composite.instances.len() > 16_384 {
            return Err(invalid("too many legacy composite events"));
        }
        let forced_end = composite.instances.iter().try_fold(0u64, |end, event| {
            event
                .n_cycles
                .and_then(|duration| event.start_cycle.checked_add(u64::from(duration)))
                .map(|at| end.max(at))
        });
        let unit = forced_end
            .filter(|end| *end > 0 && loop_.length_frames > 0 && loop_.length_frames % *end == 0)
            .map_or(source_unit, |end| loop_.length_frames / end);
        let mut timing = CompositeTimingDocument {
            basis_sample_rate: document.sample_rate,
            basis_unit_frames: unit,
            sample_rate: document.sample_rate,
            quantum_frames: 1,
            unit_frames: unit,
            iterations: 0,
            instances: Vec::new(),
            spans: Vec::new(),
        };
        let mut targets = BTreeMap::<u64, Vec<(u64, u64, u32, String)>>::new();
        for (ordinal, event) in composite.instances.iter().enumerate() {
            let length = loops
                .get(&event.loop_id)
                .ok_or_else(|| invalid("missing legacy composite child"))?;
            let duration = event
                .n_cycles
                .map(u64::from)
                .unwrap_or_else(|| length.div_ceil(unit).max(1));
            let end = event
                .start_cycle
                .checked_add(duration)
                .ok_or_else(|| invalid("composite duration overflow"))?;
            let start_frame = event
                .start_cycle
                .checked_mul(unit)
                .ok_or_else(|| invalid("composite start overflow"))?;
            let end_frame = end
                .checked_mul(unit)
                .ok_or_else(|| invalid("composite end overflow"))?;
            timing.instances.push(CompositeEffectiveInstanceDocument {
                instance_id: event.instance_id,
                start_frame,
                end_frame,
            });
            timing.iterations = timing.iterations.max(end);
            targets.entry(event.loop_id).or_default().push((
                event.start_cycle,
                end,
                ordinal as u32,
                event.mode.clone().unwrap_or_else(|| "playing".to_owned()),
            ));
        }
        for (target, events) in targets {
            let mut edges = BTreeMap::<u64, (Vec<u32>, Vec<u32>)>::new();
            let mut modes = BTreeMap::new();
            for (start, end, ordinal, mode) in events {
                edges.entry(start).or_default().1.push(ordinal);
                edges.entry(end).or_default().0.push(ordinal);
                modes.insert(ordinal, mode);
            }
            let mut active = BTreeSet::new();
            let mut previous = None;
            for (at, (ends, starts)) in edges {
                if let Some(start) = previous {
                    if let Some(&ordinal) = active.last() {
                        let mode = &modes[&ordinal];
                        if let Some(span) = timing.spans.last_mut().filter(|span| {
                            span.target == target && span.end == start && span.mode == *mode
                        }) {
                            span.end = at;
                        } else {
                            timing.spans.push(CompositeSpanDocument {
                                target,
                                start,
                                end: at,
                                occurrence_start: start,
                                ordinal,
                                mode: mode.clone(),
                            });
                        }
                    }
                }
                for ordinal in ends {
                    active.remove(&ordinal);
                }
                active.extend(starts);
                previous = Some(at);
            }
        }
        loop_.length_frames = unit
            .checked_mul(timing.iterations)
            .ok_or_else(|| invalid("composite length overflow"))?;
        composite.timing = Some(timing);
    }
    Ok(())
}

impl CompositeTimingDocument {
    pub fn converted(&self, target: ConversionTarget) -> Result<Self, SessionError> {
        if !target.is_valid() || self.sample_rate == 0 || self.unit_frames == 0 {
            return Err(invalid("invalid composite format"));
        }
        let convert = |frame: u64| {
            let numerator = u128::from(frame) * u128::from(target.sample_rate);
            let denominator = u128::from(self.sample_rate) * u128::from(target.quantum_frames);
            u64::try_from(
                (numerator + denominator / 2) / denominator * u128::from(target.quantum_frames),
            )
            .map_err(|_| invalid("converted composite endpoint overflow"))
        };
        let unit = if self.sample_rate == target.sample_rate
            && self.unit_frames % u64::from(target.quantum_frames) == 0
        {
            self.unit_frames
        } else {
            u64::from(target.quantum_frames)
        };
        let mut next = self.clone();
        next.sample_rate = target.sample_rate;
        next.quantum_frames = target.quantum_frames;
        next.unit_frames = unit;
        next.iterations = convert(
            self.unit_frames
                .checked_mul(self.iterations)
                .ok_or_else(|| invalid("composite duration overflow"))?,
        )? / unit;
        for instance in &mut next.instances {
            instance.start_frame = convert(instance.start_frame)?;
            instance.end_frame = convert(instance.end_frame)?;
            if instance.start_frame >= instance.end_frame {
                return Err(invalid("quantum conversion collapses a composite event"));
            }
        }
        for span in &mut next.spans {
            span.start = convert(
                span.start
                    .checked_mul(self.unit_frames)
                    .ok_or_else(|| invalid("composite span overflow"))?,
            )? / unit;
            span.end = convert(
                span.end
                    .checked_mul(self.unit_frames)
                    .ok_or_else(|| invalid("composite span overflow"))?,
            )? / unit;
            span.occurrence_start = span.start;
            if span.start >= span.end {
                return Err(invalid("quantum conversion collapses a composite span"));
            }
        }
        let mut normalized: Vec<CompositeSpanDocument> = Vec::new();
        for span in next.spans {
            if let Some(previous) = normalized.last_mut().filter(|previous| {
                previous.target == span.target
                    && previous.end == span.start
                    && previous.mode == span.mode
            }) {
                previous.end = span.end;
            } else {
                normalized.push(span);
            }
        }
        next.spans = normalized;
        Ok(next)
    }
}

pub(crate) fn validate_composite_documents(document: &SessionDocument) -> Result<(), SessionError> {
    let loops = document
        .track_groups
        .iter()
        .flat_map(|g| &g.tracks)
        .flat_map(|t| &t.loops)
        .map(|loop_| (loop_.id, loop_))
        .collect::<BTreeMap<_, _>>();
    let mut edges = BTreeMap::<u64, BTreeSet<u64>>::new();
    let mut quantum = None;
    for (&id, loop_) in &loops {
        if let Some(source) = loop_
            .execution
            .as_ref()
            .and_then(|execution| execution.sync_source)
        {
            if !loops.contains_key(&source) {
                return Err(invalid("missing saved synchronization source"));
            }
            edges.entry(source).or_default().insert(id);
        }
        let Some(composite) = &loop_.composite else {
            continue;
        };
        for event in &composite.instances {
            edges.entry(id).or_default().insert(event.loop_id);
        }
        let Some(timing) = &composite.timing else {
            continue;
        };
        if timing.basis_sample_rate == 0
            || timing.basis_unit_frames == 0
            || timing.sample_rate != document.sample_rate
            || !(ConversionTarget {
                sample_rate: timing.sample_rate,
                quantum_frames: timing.quantum_frames,
            })
            .is_valid()
            || timing.unit_frames == 0
            || timing.unit_frames % u64::from(timing.quantum_frames) != 0
            || timing.unit_frames.checked_mul(timing.iterations) != Some(loop_.length_frames)
            || timing.instances.len() != composite.instances.len()
            || timing.instances.len() > 16_384
            || timing.spans.len() > 32_768
            || (timing.iterations == 0) != timing.spans.is_empty()
            || timing.spans.iter().map(|span| span.end).max().unwrap_or(0) != timing.iterations
            || quantum.is_some_and(|q| q != timing.quantum_frames)
        {
            return Err(invalid("invalid saved composite timing"));
        }
        quantum = Some(timing.quantum_frames);
        for (effective, authored) in timing.instances.iter().zip(&composite.instances) {
            if effective.instance_id != authored.instance_id
                || effective.start_frame >= effective.end_frame
                || effective.end_frame > loop_.length_frames
                || effective.start_frame % timing.unit_frames != 0
                || effective.end_frame % timing.unit_frames != 0
            {
                return Err(invalid("invalid saved composite instance geometry"));
            }
        }
        let mut previous: Option<&CompositeSpanDocument> = None;
        for span in &timing.spans {
            let event = composite
                .instances
                .get(span.ordinal as usize)
                .ok_or_else(|| invalid("invalid saved composite ordinal"))?;
            if span.target != event.loop_id
                || span.mode != event.mode.as_deref().unwrap_or("playing")
                || span.start >= span.end
                || span.end > timing.iterations
                || span.occurrence_start != span.start
                || previous.is_some_and(|old| {
                    old.target > span.target
                        || old.target == span.target
                            && (old.end > span.start
                                || old.end == span.start && old.mode == span.mode)
                })
            {
                return Err(invalid("invalid saved composite spans"));
            }
            previous = Some(span);
        }
    }
    fn visit(
        id: u64,
        edges: &BTreeMap<u64, BTreeSet<u64>>,
        visiting: &mut BTreeSet<u64>,
        done: &mut BTreeSet<u64>,
    ) -> bool {
        if done.contains(&id) {
            return true;
        }
        if !visiting.insert(id) {
            return false;
        }
        if edges
            .get(&id)
            .is_some_and(|targets| targets.iter().any(|id| !visit(*id, edges, visiting, done)))
        {
            return false;
        }
        visiting.remove(&id);
        done.insert(id);
        true
    }
    let mut visiting = BTreeSet::new();
    let mut done = BTreeSet::new();
    if edges
        .keys()
        .any(|id| !visit(*id, &edges, &mut visiting, &mut done))
    {
        return Err(invalid("saved composite/synchronization dependency cycle"));
    }
    Ok(())
}
