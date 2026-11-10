//! Physical node/capture state rendezvous with the transition subsystem.
use super::*;

pub(super) enum Preparation {
    Ready,
    Retry,
}

pub(super) struct StateProcessor;
impl StateProcessor {
    pub(super) fn cancel_preparation(
        owner: &mut LoopPlanner,
        io: &mut PlannerResourceIo<'_>,
    ) -> Result<(), Error> {
        owner.cancel_ticks(io)
    }
    pub(super) fn prepare(
        owner: &mut LoopPlanner,
        frame: Frame,
        end: Frame,
        io: &mut PlannerResourceIo<'_>,
    ) -> Result<Preparation, Error> {
        Ok(if owner.preflight(frame, end, io)? {
            Preparation::Ready
        } else {
            Preparation::Retry
        })
    }
    pub(super) fn apply(
        owner: &mut LoopPlanner,
        frame: Frame,
        io: &mut PlannerResourceIo<'_>,
    ) -> Result<(), Error> {
        owner.settle(frame, io)
    }
    pub(super) fn finish(
        owner: &mut LoopPlanner,
        context: &mut FinishContext<'_, Operation, Reply>,
    ) -> Result<(), Error> {
        for row in owner.rows.iter_mut().flatten().filter(|row| row.assigned) {
            if let Some(interval) = row.interval {
                if row.aborted {
                    context.resources.capture().cancel_tick(interval)?;
                } else {
                    context.resources.capture().commit(interval)?;
                }
                row.interval = None;
            }
            row.reset_input = false;
        }
        Ok(())
    }
}
