use super::{apply_plan, loop_state};
use crate::host::{Client, Error, RegisterBus, Request, registers::*};
use alloc::vec::Vec;
use rt_fpga_compiler::Identity;
use rt_fpga_reference::transitions::LoopId;
use shoop_rt_protocol::immediate::{Code, TransitionPlan};

struct Edit {
    id: Identity,
    original: [u64; 16],
    updated: [u64; 16],
    self_sync: bool,
}

/// Prepared edits are uploaded without holding the processing boundary.
/// The fabric validates every guard before applying any masked write.
pub struct LoopBatch {
    edits: Vec<Edit>,
    control_cycles: Option<u32>,
}
impl Default for LoopBatch {
    fn default() -> Self {
        Self::new()
    }
}
impl LoopBatch {
    pub fn new() -> Self {
        Self {
            edits: Vec::with_capacity(32),
            control_cycles: None,
        }
    }
    pub fn clear(&mut self) {
        self.edits.clear();
        self.control_cycles = None;
    }
    pub fn control_cycles(&self) -> Option<u32> {
        self.control_cycles
    }

    pub fn add_future(
        &mut self,
        id: Identity,
        source: Option<LoopId>,
        words: [u64; 16],
        frame: u64,
        plan: TransitionPlan,
    ) -> Result<Code, Code> {
        if plan.now.is_some() {
            return Err(Code::Unsupported);
        }
        if id.slot > 4095 || words[0] & 1 == 0 || (words[0] >> 1) as u32 != id.generation {
            return Err(Code::TargetUnavailable);
        }
        let index = self.edits.iter().position(|e| e.id == id);
        if index.is_none() && self.edits.len() == 32 {
            return Err(Code::CannotApply);
        }
        let mut updated = index.map_or(words, |i| self.edits[i].updated);
        let state = loop_state(id, &updated, None);
        let code = apply_plan(&mut updated, state, source, frame, plan)?;
        if let Some(index) = index {
            self.edits[index].updated = updated;
        } else {
            self.edits.push(Edit {
                id,
                original: words,
                updated,
                self_sync: source
                    .is_none_or(|s| s.slot == id.slot && s.generation == id.generation),
            });
        }
        Ok(code)
    }

    pub fn commit<B: RegisterBus>(
        &mut self,
        client: &mut Client<B>,
        revision: u64,
        frame: u64,
        commands: u32,
        deadline: Option<u64>,
        mut time_left: impl FnMut() -> bool,
    ) -> Result<(u64, Code), Error<B::Error>> {
        self.control_cycles = None;
        self.edits.sort_unstable_by_key(|e| e.id.slot);
        let count = self.edits.iter().flat_map(patches).count();
        if count == 0
            || count > 512
            || commands == 0
            || commands > 32
            || count > commands as usize * 16
        {
            return Err(Error::InvalidRequest);
        }
        let mut send = |request| {
            let reply = client.exchange(request, &mut time_left)?;
            if reply.code != RESULT_OK {
                return Err(Error::Rejected(reply.code));
            }
            Ok(())
        };
        send(Request::begin_batch(
            revision,
            frame,
            count as u32,
            commands,
            deadline,
        ))?;
        for patch in self.edits.iter().flat_map(patches) {
            send(patch)?;
        }
        let reply = client.exchange(Request::commit_batch(), time_left)?;
        self.control_cycles = (reply.code == RESULT_OK).then_some(reply.data[5]);
        let selected = u64::from(reply.data[0]) | u64::from(reply.data[1]) << 32;
        match reply.code {
            RESULT_OK
                if selected >= frame
                    && (u64::from(reply.data[2]) | u64::from(reply.data[3]) << 32) == revision =>
            {
                Ok((selected, Code::Applied))
            }
            RESULT_REJECTED => Ok((
                selected,
                match reply.data[4] {
                    1 => Code::DeadlineMissed,
                    2 => Code::StaleConfiguration,
                    _ => Code::CannotApply,
                },
            )),
            RESULT_FAULT => Err(Error::Fault),
            _ => Err(Error::UnexpectedResponse),
        }
    }
}

fn patches(edit: &Edit) -> impl Iterator<Item = Request> + '_ {
    let transport = (edit.original[0] ^ edit.updated[0]) & !(1 << 56) != 0
        || edit.original[2..8] != edit.updated[2..8];
    (0..16).filter_map(move |i| {
        let mask = match i {
            0 => (edit.original[0] ^ edit.updated[0]) | (1 << 56),
            1 => 0x3ff << 32,
            10..=15 => u64::MAX,
            _ if edit.original[i] != edit.updated[i] => u64::MAX,
            _ => 0,
        };
        let guard = match i {
            0 if !transport => (1u64 << 33) - 1 | if edit.self_sync { 3 << 33 } else { 0 },
            1 if !transport => 0xffffffff | super::COMPOSITE_LOOP | (1 << 44),
            0..=7 if transport => u64::MAX,
            _ => 0,
        };
        if mask == 0 && guard == 0 {
            return None;
        }
        let rebase = matches!(i, 10 | 13)
            && edit.updated[1] & (1 << (32 + if i == 10 { 0 } else { 5 })) != 0;
        Request::stage_patch(
            edit.id.slot * 16 + i as u32,
            edit.original[i],
            guard,
            mask,
            edit.updated[i],
            rebase,
        )
    })
}
