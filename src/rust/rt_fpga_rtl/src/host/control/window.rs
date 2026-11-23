use super::{
    apply_iteration, apply_plan, clear_plan, gain_value, loop_state, session::FencedSwitch,
    set_repeat_sync,
};
use crate::host::{Client, Error, RegisterBus, Request, registers::*};
use crate::target::{ConfigurationWrite, Space};
use rt_fpga_compiler::Identity;
use rt_fpga_reference::transitions::LoopId;
use shoop_rt_protocol::immediate::{
    ClearPlanned, Code, IterationTransition, SwitchProgram, TransitionPlan,
};

/// A checked boundary for modifying active state. The PL faults if the owner does not release it.
pub struct ControlWindow<'a, B, F> {
    client: &'a mut Client<B>,
    time_left: F,
    frame: u64,
    revision: u64,
    composite_wrap: bool,
}

impl<'a, B: RegisterBus, F: FnMut() -> bool> ControlWindow<'a, B, F> {
    pub fn begin(
        client: &'a mut Client<B>,
        revision: u64,
        time_left: F,
    ) -> Result<Self, Error<B::Error>> {
        let mut window = Self {
            client,
            time_left,
            frame: 0,
            revision,
            composite_wrap: false,
        };
        let data = window.send(Request::begin_control(revision))?;
        window.frame = join(data[0], data[1]);
        if join(data[2], data[3]) != revision {
            return Err(Error::UnexpectedResponse);
        }
        Ok(window)
    }

    pub fn frame(&self) -> u64 {
        self.frame
    }

    pub fn release(mut self) -> Result<(), Error<B::Error>> {
        self.send(Request::end_control()).map(|_| ())
    }

    pub fn switch_program(
        &mut self,
        program: SwitchProgram,
    ) -> Result<FencedSwitch, Error<B::Error>> {
        let words = self.send(Request::switch_program(self.revision, program))?;
        let fence = FencedSwitch::new(program, words, self.composite_wrap);
        if fence.frame() != self.frame {
            return Err(Error::UnexpectedResponse);
        }
        Ok(fence)
    }

    pub fn arm_future_switch(
        &mut self,
        program: SwitchProgram,
        cut: rt_fpga_compiler::loading::FutureSwitch,
    ) -> Result<(), Error<B::Error>> {
        let words = self.send(Request::switch_at_resolved_position(
            self.revision,
            program,
            cut,
        ))?;
        let reply = FencedSwitch::new(program, words, false);
        if reply.frame() != self.frame || reply.revision() != self.revision {
            return Err(Error::UnexpectedResponse);
        }
        Ok(())
    }

    pub fn composite_cut_ready(
        &mut self,
        cut: &rt_fpga_compiler::loading::CompositeCut,
    ) -> Result<bool, Error<B::Error>> {
        Ok(self.composite_cut_status(cut)? == Code::Applied)
    }

    pub fn composite_cut_status(
        &mut self,
        cut: &rt_fpga_compiler::loading::CompositeCut,
    ) -> Result<Code, Error<B::Error>> {
        self.composite_cut(cut, None)
    }

    pub fn prepare_composite_cut(
        &mut self,
        cut: &rt_fpga_compiler::loading::CompositeCut,
        quantum: u32,
    ) -> Result<Code, Error<B::Error>> {
        self.composite_cut(cut, Some(quantum))
    }

    fn composite_cut(
        &mut self,
        cut: &rt_fpga_compiler::loading::CompositeCut,
        quantum: Option<u32>,
    ) -> Result<Code, Error<B::Error>> {
        let (_, Some(words)) = self.read_loop(cut.old_slot)? else {
            return Ok(Code::CannotApply);
        };
        let status = super::composite_cut_status(cut, &words, |id| {
            let latch = self.capture_loop(id)?;
            let words = self.read_latched_words::<4>(latch, 0)?;
            super::restart_clock_state(id, self.frame, words, |offset| {
                self.read_latched_words::<2>(latch, offset)
            })
        })?;
        if status != Code::Applied || (words[0] >> 33) & 3 != 1 {
            return Ok(status);
        }
        let Some(quantum) = quantum else {
            return Ok(status);
        };
        if let Some(next) = cut.restart_slot {
            let (base, Some(original)) = self.read_loop(next)? else {
                return Ok(Code::CannotApply);
            };
            let mut updated = original;
            let code = super::prepare_composite_restart(next, &words, &mut updated, quantum);
            if code != Code::Applied {
                return Ok(code);
            }
            self.update_loop(base, original, updated)?;
        }
        self.composite_wrap |= cut.reconciles_wrap(words[2], words[3], words[1] & (1 << 42) != 0);
        Ok(status)
    }

    pub fn apply_transition(
        &mut self,
        id: Identity,
        source: Option<LoopId>,
        last_pulse: Option<u64>,
        plan: TransitionPlan,
    ) -> Result<Code, Error<B::Error>> {
        let (base, original) = self.read_loop(id)?;
        let Some(original) = original else {
            return Ok(Code::TargetUnavailable);
        };
        let mut updated = original;
        let state = loop_state(id, &original, last_pulse);
        let code = match apply_plan(&mut updated, state, source, self.frame, plan) {
            Ok(code) => code,
            Err(code) => return Ok(code),
        };
        self.update_loop(base, original, updated)?;
        Ok(code)
    }

    pub fn prepare_composite_retirement(&mut self, id: Identity) -> Result<Code, Error<B::Error>> {
        let (base, Some(original)) = self.read_loop(id)? else {
            return Ok(Code::CannotApply);
        };
        let mut updated = original;
        let code = super::prepare_composite_retirement(id, &mut updated, self.frame);
        if code == Code::Scheduled {
            self.update_loop(base, original, updated)?;
        }
        Ok(code)
    }

    pub fn clear_transitions(
        &mut self,
        id: Identity,
        mask: ClearPlanned,
    ) -> Result<Code, Error<B::Error>> {
        let (base, original) = self.read_loop(id)?;
        let Some(original) = original else {
            return Ok(Code::TargetUnavailable);
        };
        let mut updated = original;
        clear_plan(&mut updated, mask);
        self.update_loop(base, original, updated)?;
        Ok(Code::Applied)
    }

    pub fn apply_iteration(
        &mut self,
        id: Identity,
        request: IterationTransition,
    ) -> Result<Code, Error<B::Error>> {
        let (base, original) = self.read_loop(id)?;
        let Some(original) = original else {
            return Ok(Code::TargetUnavailable);
        };
        let mut updated = original;
        let state = loop_state(id, &original, None);
        let code = match apply_iteration(&mut updated, state, self.frame, request) {
            Ok(code) => code,
            Err(code) => return Ok(code),
        };
        self.update_loop(base, original, updated)?;
        Ok(code)
    }

    pub fn set_repeat_sync(
        &mut self,
        id: Identity,
        enabled: bool,
    ) -> Result<Code, Error<B::Error>> {
        let (base, original) = self.read_loop(id)?;
        let Some(original) = original else {
            return Ok(Code::TargetUnavailable);
        };
        let mut updated = original;
        set_repeat_sync(&mut updated, enabled);
        self.update_loop(base, original, updated)?;
        Ok(Code::Applied)
    }

    pub fn set_gain(&mut self, address: u32, gain: f32) -> Result<(), Error<B::Error>> {
        let write = ConfigurationWrite {
            space: Space::State,
            address: u64::from(address),
            value: gain_value(gain),
        };
        self.send(Request::configuration_write(&write).ok_or(Error::InvalidRequest)?)?;
        Ok(())
    }

    fn read_loop(&mut self, id: Identity) -> Result<(u32, Option<[u64; 16]>), Error<B::Error>> {
        let latch = self.capture_loop(id)?;
        let row = self.read_latched_words::<16>(latch, 0)?;
        let current = row[0] & 1 != 0 && (row[0] >> 1) as u32 == id.generation;
        Ok((id.slot * 16, current.then_some(row)))
    }

    fn capture_loop(&mut self, id: Identity) -> Result<u32, Error<B::Error>> {
        if id.slot > 4095 {
            return Err(Error::InvalidRequest);
        }
        let snapshot = self.send(Request::capture_loop(id.slot))?;
        if join(snapshot[1], snapshot[2]) != self.frame
            || join(snapshot[3], snapshot[4]) != self.revision
        {
            return Err(Error::UnexpectedResponse);
        }
        Ok(snapshot[0])
    }

    fn read_latched_words<const N: usize>(
        &mut self,
        latch: u32,
        offset: u32,
    ) -> Result<[u64; N], Error<B::Error>> {
        let mut row = [0; N];
        for (index, words) in row.chunks_mut(HOST_PAYLOAD_WORDS / 2).enumerate() {
            let address = offset + (index * (HOST_PAYLOAD_WORDS / 2)) as u32;
            let data = self.send(Request::read_snapshot(latch, address, words.len() as u32))?;
            for (word, parts) in words.iter_mut().zip(data.chunks_exact(2)) {
                *word = join(parts[0], parts[1]);
            }
        }
        Ok(row)
    }

    fn update_loop(
        &mut self,
        base: u32,
        original: [u64; 16],
        updated: [u64; 16],
    ) -> Result<(), Error<B::Error>> {
        for (index, (before, after)) in original.into_iter().zip(updated).enumerate() {
            if before != after {
                self.send(Request::write_loop_word(base + index as u32, after))?;
            }
        }
        Ok(())
    }

    fn send(&mut self, request: Request) -> Result<[u32; HOST_PAYLOAD_WORDS], Error<B::Error>> {
        let response = self.client.exchange(request, &mut self.time_left)?;
        if response.code != RESULT_OK {
            return Err(Error::Rejected(response.code));
        }
        Ok(response.data)
    }
}

fn join(low: u32, high: u32) -> u64 {
    u64::from(low) | u64::from(high) << 32
}
