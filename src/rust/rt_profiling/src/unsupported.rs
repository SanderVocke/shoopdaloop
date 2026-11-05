use crate::{Clock, Packet, Phase, Status};

#[derive(Clone)]
pub struct Control;
impl Control {
    pub fn use_outer_callback_budget(&self) {}
    pub fn recording(&self) -> bool {
        false
    }
    pub fn host_begin(&self, _frame: u64, _frames: u32, _at: u64) {}
    pub fn host_begin_external(&self, _frame: u64, _callback_frame: u64, _frames: u32, _at: u64) {}
    pub fn host_input(&self, _at: u64) {}
    pub fn host_end(&self, _at: u64, _budget_ns: u64) {}
    pub fn set_enabled(&self, _enabled: bool) {}
    pub fn reset(&self) {}
    pub fn trigger(&self, _cause: u32) {}
    pub fn status(&self) -> Status {
        Status::default()
    }
}
#[derive(Default)]
pub struct Recorder;
impl Recorder {
    pub fn set_clock(&mut self, _clock: Clock) {}
    pub fn enabled(&self) -> bool {
        false
    }
    pub fn begin_callback(&mut self, _frame: u64, _epoch: u64, _frames: u32, _budget_ns: u64) {}
    pub fn callback_phase(&mut self, _phase: Phase) {}
    pub fn begin_tick(&mut self, _frame: u64, _frames: u32) {}
    pub fn graph_start(&mut self, _epoch: u64, _frame: u64) {}
    pub fn needs_layout(&self) -> bool {
        false
    }
    pub fn layout_node(&mut self, _node: u64) {}
    pub fn layout_done(&mut self) {}
    pub fn phase(&mut self, _phase: Phase) {}
    pub fn node_ready(&mut self, _node: u64) {}
    pub fn end_tick(&mut self, _complete: bool) {}
    pub fn end_callback(&mut self, _complete: bool) {}
}
pub struct Reader {
    control: Control,
}
impl Reader {
    pub fn control(&self) -> &Control {
        &self.control
    }
    pub fn next_packet(&mut self) -> Option<Packet> {
        None
    }
}
pub fn prepare(_max_nodes: usize) -> Option<(Control, Recorder, Reader)> {
    None
}
