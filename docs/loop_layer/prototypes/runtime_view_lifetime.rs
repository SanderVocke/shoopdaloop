use rt_runtime::planner::{NoPlanner, PlanContext, Planner};
use rt_runtime::resource::ResourceStore;
use rt_runtime::{Config, Error, Frame};

fn settle<L: Planner>(
    planner: &mut L,
    context: PlanContext<'_, L::Operation, L::Result>,
) -> Result<Frame, Error> {
    let decisions = planner.decisions();
    #[cfg(compile_fail_reuse)]
    {
        let end = planner.plan(context)?;
        std::hint::black_box(decisions);
        Ok(end)
    }
    #[cfg(not(compile_fail_reuse))]
    {
        std::hint::black_box(decisions);
        planner.plan(context)
    }
}

fn main() {
    let mut planner = NoPlanner;
    let (_results, mut scheduler) =
        rt_runtime::schedule::scheduler(Config::default().prepare().unwrap()).unwrap();
    let mut resources = ResourceStore::new(Config::default().prepare().unwrap()).unwrap();
    let end = settle(
        &mut planner,
        PlanContext {
            frame: Frame(0),
            ceiling: Frame(128),
            resources: resources.io(),
            receipts: scheduler.receipts(Frame(0)),
        },
    )
    .unwrap();
    assert_eq!(end, Frame(128));
}
