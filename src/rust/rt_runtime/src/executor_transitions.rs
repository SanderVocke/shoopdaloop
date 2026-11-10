//! Transition/timing order is independent of the compiled signal program.
use super::*;

struct TransitionProcessor<'a, P, L: Planner, Q: ProcessorOperation> {
    runtime: &'a mut GraphRuntime<P, L, Q>,
}
impl<P: Processor<L, Q>, L: Planner, Q: ProcessorOperation> GraphRuntime<P, L, Q> {
    pub(super) fn begin_fpga_transitions(
        &mut self,
        frame: Frame,
        end: Frame,
        callback_start: Frame,
        remaining: &mut usize,
    ) -> Result<(), Error> {
        TransitionProcessor { runtime: self }.begin(frame, end, callback_start, remaining)
    }
    pub(super) fn finish_fpga_transitions(
        &mut self,
        frame: Frame,
        end: Frame,
    ) -> Result<(), Error> {
        TransitionProcessor { runtime: self }.finish(frame, end)
    }
}
impl<P: Processor<L, Q>, L: Planner, Q: ProcessorOperation> TransitionProcessor<'_, P, L, Q> {
    fn begin(
        &mut self,
        frame: Frame,
        end: Frame,
        callback_start: Frame,
        remaining: &mut usize,
    ) -> Result<(), Error> {
        let runtime = &mut self.runtime;
        runtime.deliver_actions(frame, callback_start, remaining)?;
        runtime.planner.begin_transitions(PlanContext {
            graph_epoch: runtime.epoch,
            graphs: GraphAccess::new(
                &mut runtime.graph_stages,
                &mut runtime.active,
                &mut runtime.epoch,
                true,
                &|candidate| activation_compatible(&runtime.slots, candidate),
                &mut runtime.recovery,
            ),
            callback_start,
            frame,
            ceiling: end,
            resources: runtime.resources.planner_io(frame, end),
            receipts: runtime.scheduler.receipts(frame),
        })?;
        if runtime.scheduler.recovery_pending() {
            return Err(Error::InvalidState);
        }
        runtime.profiling.phase(rt_profiling::Phase::StateSettled);
        runtime.profiling.graph_start(runtime.epoch.0, frame.0);
        Ok(())
    }
    fn finish(&mut self, frame: Frame, end: Frame) -> Result<(), Error> {
        let runtime = &mut self.runtime;
        runtime.planner.finish_transitions(FinishContext {
            interval: Interval { start: frame, end },
            resources: runtime.resources.planner_io(end, end),
            receipts: runtime.scheduler.receipts(end),
        })
    }
}
