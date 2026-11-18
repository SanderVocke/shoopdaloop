use super::{apply_plan, clear_plan, gain_value, loop_state};
use crate::host::{Client, Error, RegisterBus, Request, registers::*};
use crate::target::{ConfigurationWrite, Space};
use rt_fpga_compiler::Identity;
use rt_fpga_reference::transitions::LoopId;
use shoop_rt_protocol::immediate::{ClearPlanned, Code, TransitionPlan};

/// A checked boundary for modifying active state. The PL faults if the owner does not release it.
pub struct ControlWindow<'a, B, F> {
    client: &'a mut Client<B>,
    time_left: F,
    frame: u64,
    revision: u64,
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
        let Some(base) = id.slot.checked_mul(16).filter(|&base| base <= 65520) else {
            return Err(Error::InvalidRequest);
        };
        let snapshot = self.send(Request::capture_snapshot(id.slot + 1))?;
        if join(snapshot[1], snapshot[2]) != self.frame
            || join(snapshot[3], snapshot[4]) != self.revision
        {
            return Err(Error::UnexpectedResponse);
        }
        let mut row = [0; 16];
        for (index, words) in row.chunks_mut(HOST_PAYLOAD_WORDS / 2).enumerate() {
            let address = base + (index * (HOST_PAYLOAD_WORDS / 2)) as u32;
            let data = self.send(Request::read_snapshot(
                snapshot[0],
                address,
                words.len() as u32,
            ))?;
            for (word, parts) in words.iter_mut().zip(data.chunks_exact(2)) {
                *word = join(parts[0], parts[1]);
            }
        }
        let current = row[0] & 1 != 0 && (row[0] >> 1) as u32 == id.generation;
        Ok((base, current.then_some(row)))
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
