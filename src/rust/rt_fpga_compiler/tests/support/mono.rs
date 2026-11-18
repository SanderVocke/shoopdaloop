use rt_fpga_compiler::shoop_graph::{Graph, Id};
#[derive(Clone, Copy)]
pub struct MonoLoop {
    pub id: Id,
    pub source: Option<u32>,
    pub input: bool,
    pub output: bool,
    pub gain: f32,
    pub repeat_sync: bool,
}
pub fn session(loops: &[MonoLoop]) -> Graph {
    let records = loops
        .iter()
        .map(|l| shoop_rt_protocol::immediate::GraphLoop {
            slot: l.id.slot,
            generation: l.id.generation,
            source: l.source,
            input: l.input.then_some(0),
            output: l.output,
            gain: l.gain,
            repeat_sync: l.repeat_sync,
        })
        .collect::<Vec<_>>();
    shoop_rt_protocol::graph::from_loops(&records)
}
