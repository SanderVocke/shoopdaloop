//! Physical node/capture state rendezvous with the transition subsystem.
use super::*;
use rt_runtime::Sample;

pub(super) enum Preparation {
    Ready,
    Retry,
}

pub(super) struct StateProcessor;
impl StateProcessor {
    pub(super) fn cancel_preparation<S: Sample>(
        owner: &mut LoopPlanner<S>,
        io: &mut PlannerResourceIo<'_, S>,
    ) -> Result<(), Error> {
        owner.cancel_ticks(io)
    }
    pub(super) fn prepare<S: Sample>(
        owner: &mut LoopPlanner<S>,
        frame: Frame,
        end: Frame,
        io: &mut PlannerResourceIo<'_, S>,
    ) -> Result<Preparation, Error> {
        Ok(if owner.preflight(frame, end, io)? {
            Preparation::Ready
        } else {
            Preparation::Retry
        })
    }
    pub(super) fn apply<S: Sample>(
        owner: &mut LoopPlanner<S>,
        frame: Frame,
        io: &mut PlannerResourceIo<'_, S>,
    ) -> Result<(), Error> {
        owner.settle(frame, io)
    }
    pub(super) fn finish<S: Sample>(
        owner: &mut LoopPlanner<S>,
        context: &mut FinishContext<'_, Operation<S>, Reply<S>, S>,
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
