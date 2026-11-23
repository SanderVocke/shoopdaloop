//! ARM memory writes and the ordered switch-command bridge. Pacing belongs to the driver.
use crate::{bits::Bits, clock::Edges, model::Model, sim::Agent, target::ProgramImage};
use shoop_rt_protocol::immediate::{Code, Command, Header, SwitchProgram};
use shoop_rt_transport::rt;

struct Write {
    history: bool,
    loop_state: bool,
    state: bool,
    address: u64,
    value: Bits,
}
pub struct Upload {
    writes: Vec<Write>,
    index: usize,
    waiting: bool,
    wait_for_execution: bool,
    pub accepted_during_execution: usize,
}
impl Upload {
    pub fn new(image: &ProgramImage, region_depth: u64) -> Self {
        let writes = image
            .writes(region_depth)
            .into_iter()
            .map(|w| {
                use rt_fpga_rtl::target::Space;
                Write {
                    history: w.space == Space::History,
                    loop_state: w.space == Space::Loop,
                    state: w.space == Space::State,
                    address: w.address,
                    value: w.value,
                }
            })
            .collect();
        Self {
            writes,
            index: 0,
            waiting: false,
            wait_for_execution: false,
            accepted_during_execution: 0,
        }
    }
    pub fn complete(&self) -> bool {
        self.index == self.writes.len()
    }
    pub fn during_execution(&mut self) {
        self.wait_for_execution = true;
    }
}
impl Agent for Upload {
    fn drive(&mut self, m: &mut Model<'_>, e: Edges) {
        if !e.falls(0) || self.complete() {
            return;
        }
        m.set("cfg_valid", 0);
        if self.wait_for_execution && self.index == 0 && m.get("busy") == 0 {
            return;
        }
        let Some(w) = self.writes.get(self.index) else {
            return;
        };
        if w.history {
            m.set("audio_cfg_valid", 0);
        }
        if self.waiting {
            return;
        }
        if w.history {
            m.set_all(&[
                ("audio_cfg_valid", 1),
                ("audio_cfg_space", 3),
                ("audio_cfg_write", 1),
                ("audio_cfg_address", w.address),
            ]);
            m.set_bits("audio_cfg_write_data", &w.value);
        } else {
            m.set_all(&[
                ("cfg_valid", 1),
                ("cfg_loop", u64::from(w.loop_state)),
                ("cfg_state", u64::from(w.state)),
                ("cfg_write", 1),
                ("cfg_address", w.address),
            ]);
            m.set_bits("cfg_write_data", &w.value);
        }
    }
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if !e.rises(0) || self.complete() {
            return;
        }
        let prefix = if self.writes[self.index].history {
            "audio_cfg"
        } else {
            "cfg"
        };
        if self.waiting {
            if m.get(&format!("{prefix}_response_valid")) != 0 {
                assert_eq!(m.get(&format!("{prefix}_response_error")), 0);
                self.index += 1;
                self.waiting = false;
            }
        } else if m.get(&format!("{prefix}_valid")) != 0 && m.get(&format!("{prefix}_ready")) != 0 {
            self.waiting = true;
            self.accepted_during_execution += usize::from(m.get("busy") != 0);
        }
    }
}

pub fn descriptor(image: &ProgramImage) -> SwitchProgram {
    let count = |n: usize| u16::try_from(n).expect("target capacity");
    SwitchProgram {
        revision: image.layout.revision,
        region: u16::from(image.layout.region),
        instructions: count(image.compiled.instructions.len()),
        sync_count: count(image.compiled.sync_count),
        states: count(image.compiled.state.len()),
        buffers: count(image.compiled.resources.buffers as usize),
    }
}

/// One published group stays unread, and its response credit remains reserved, until the hardware fence completes.
#[derive(Default)]
pub struct Commands {
    pending: Option<Header>,
}
impl Commands {
    pub fn boundary(&mut self, m: &mut Model<'_>, execution: &mut rt::Execution, frame: u64) {
        m.set("switch_response_ready", 0);
        execution.commands.begin_callback();
        if self.pending.is_some() && m.get("switch_response_valid") == 0 {
            return;
        }
        m.set("switch_valid", 0);
        let Some(group) = execution.commands.pop() else {
            return;
        };
        let header = group.header;
        if let Some(pending) = self.pending.take() {
            assert_eq!(pending, header);
            let code = if m.get("switch_response_error") != 0 {
                Code::CannotApply
            } else {
                Code::Applied
            };
            let frame = m.get("switch_first_frame");
            group.respond(header.complete(frame, code), |record| record.complete(code));
            m.set("switch_response_ready", 1);
            return;
        }
        let mut records = group.commands();
        let record = records.next().copied();
        let multiple = records.next().is_some();
        drop(records);
        let code = if header.graph() != m.get("active_revision")
            || header.topology() != m.get("active_revision")
        {
            Some(Code::StaleConfiguration)
        } else if header.deadline().is_some_and(|deadline| frame > deadline) {
            Some(Code::DeadlineMissed)
        } else if multiple
            || !record.is_some_and(|r| matches!(r.command(), Command::SwitchProgram(_)))
        {
            Some(Code::CannotApply)
        } else {
            None
        };
        if let Some(code) = code {
            group.respond(header.complete(frame, code), |r| r.complete(code));
            return;
        }
        let Command::SwitchProgram(d) = record.unwrap().command() else {
            unreachable!()
        };
        m.set_all(&[
            ("switch_valid", 1),
            ("switch_expected", header.graph()),
            ("switch_revision", d.revision),
            ("switch_region", u64::from(d.region)),
            ("switch_instructions", u64::from(d.instructions)),
            ("switch_sync_count", u64::from(d.sync_count)),
            ("switch_states", u64::from(d.states)),
            ("switch_buffers", u64::from(d.buffers)),
        ]);
        self.pending = Some(header);
        group.leave_unread();
    }
    pub fn selected(&mut self, m: &mut Model<'_>) {
        if m.get("active_revision") != m.get("switch_expected")
            || m.get("switch_response_valid") != 0
        {
            m.set("switch_valid", 0);
        }
    }
}
