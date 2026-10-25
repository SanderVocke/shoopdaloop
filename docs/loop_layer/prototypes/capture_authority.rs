use rt_runtime::resource::{PlannerResourceIo, ResourceIo, ResourceStore};
use rt_runtime::{Config, Frame};

fn boundary(mut resources: PlannerResourceIo<'_>) {
    let _capability = resources.capture();
}
fn consumer(resources: &mut ResourceIo<'_>) {
    #[cfg(compile_fail_capture)]
    let _forbidden = resources.capture();
    let _data = resources.render();
}
fn main() {
    let mut resources = ResourceStore::new(Config::default().prepare().unwrap()).unwrap();
    boundary(resources.planner_io(Frame(0), Frame(1)));
    consumer(&mut resources.io());
}
