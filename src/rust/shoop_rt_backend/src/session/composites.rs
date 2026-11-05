use alloc::vec::Vec;
use anyhow::{anyhow, Result};
use shoop_backend::{
    BackendCompositeId, BackendCompositeKind, BackendCompositeTarget, BackendLoopId,
    BackendSessionData, BackendSessionReplacement,
};
use shoop_rt_protocol::{CompositeDescription, CompositeKind, CompositeSpan, ReplacementEdit};
use shoop_rt_transport::carrier::Carrier;

use crate::{mirror::Mirror, EngineClock, RtBackend};

impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    pub(super) fn replacement_composites(
        &mut self,
        session: &BackendSessionData,
        mapping: &BackendSessionReplacement,
        candidate: &mut Mirror,
    ) -> Result<Vec<ReplacementEdit>> {
        let mapped = |source| {
            mapping
                .loops
                .get(&source)
                .copied()
                .ok_or_else(|| anyhow!("unknown composite loop reference"))
        };
        let mut plans = Vec::new();
        for composite in &session.composites {
            let id = mapped(composite.source_id)?;
            let mut plan = composite.plan.clone();
            plan.sync_source = plan.sync_source.map(|id| mapped(id.raw())).transpose()?;
            for span in &mut plan.spans {
                span.target = match span.target {
                    BackendCompositeTarget::Loop(id) => {
                        BackendCompositeTarget::Loop(mapped(id.raw())?)
                    }
                    BackendCompositeTarget::Composite(id) => BackendCompositeTarget::Composite(
                        BackendCompositeId::from_raw(mapped(id.raw())?.raw()),
                    ),
                };
            }
            self.composites.next_version = self
                .composites
                .next_version
                .checked_add(1)
                .ok_or_else(|| anyhow!("composite version space exhausted"))?;
            let entry = &candidate.loops[&id];
            let d = CompositeDescription {
                id: 0,
                loop_id: entry.slot,
                sync: plan.sync_source.map(|source| candidate.loops[&source].slot),
                previous: None,
                expected_version: 0,
                version: self.composites.next_version,
                kind: match plan.kind {
                    BackendCompositeKind::Regular => CompositeKind::Regular,
                    BackendCompositeKind::Script => CompositeKind::Explicit,
                },
                unit_frames: plan.unit_frames,
                iterations: plan.iterations,
                entries: plan.entries,
                segments: u32::try_from(plan.spans.len())?,
                sample_rate: session.sample_rate,
                quantum: self.limits().quantum,
            };
            let mut spans = plan
                .spans
                .iter()
                .map(|span| {
                    let child = match span.target {
                        BackendCompositeTarget::Loop(id) => id,
                        BackendCompositeTarget::Composite(id) => BackendLoopId::from_raw(id.raw()),
                    };
                    Ok(CompositeSpan {
                        target: candidate.loops[&child].slot,
                        start: span.start,
                        end: span.end,
                        occurrence_start: span.occurrence_start,
                        ordinal: span.ordinal,
                        mode: crate::composites::wire_mode(span.mode)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            spans.sort_by_key(|span| (span.target, span.start));
            let entry = candidate.loops.get_mut(&id).unwrap();
            entry.program = Some(d);
            entry.program_spans = spans;
            entry.view.length = plan
                .unit_frames
                .checked_mul(plan.iterations)
                .ok_or_else(|| anyhow!("composite duration overflow"))?;
            plans.push((id, plan));
        }
        core::mem::swap(&mut self.mirror, candidate);
        let validated = plans
            .iter()
            .try_for_each(|(id, plan)| self.validate_prepared(*id, plan));
        core::mem::swap(&mut self.mirror, candidate);
        validated?;
        let mut edits = Vec::new();
        for (id, _) in &plans {
            edits.push(ReplacementEdit::Composite(
                candidate.loops[id].program.unwrap(),
            ));
        }
        for (id, _) in &plans {
            let entry = &candidate.loops[id];
            for (index, span) in entry.program_spans.iter().copied().enumerate() {
                edits.push(ReplacementEdit::CompositeSpan {
                    loop_id: entry.slot,
                    index: index as u32,
                    span,
                });
            }
        }
        Ok(edits)
    }
}
