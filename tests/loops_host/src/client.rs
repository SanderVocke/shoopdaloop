//! Control-instance credit ownership; requests and responses are plain byte arrays.
use crate::wire::{self, Error, Phase, Request, Response, Status, NORMAL_SLOTS, SLOTS};
use shoop_loops::{LoopId, Transition};

#[derive(Clone, Copy)]
struct Pending {
    request: Request,
    admission: Option<Response>,
}

pub struct Client {
    session: u64,
    next_sequence: Option<u64>,
    pending: [Option<Pending>; SLOTS],
}
impl Client {
    pub fn new(session: u64) -> Result<Self, Error> {
        if session == 0 {
            return Err(Error::Session);
        }
        Ok(Self {
            session,
            next_sequence: Some(1),
            pending: [None; SLOTS],
        })
    }
    /// Reserve one wire credit until rejection/cancellation or the terminal receipt.
    pub fn prepare(
        &mut self,
        at: u64,
        target: LoopId,
        transition: Transition,
        recovery: bool,
        cancelled: bool,
    ) -> Result<[u8; wire::REQUEST_BYTES], Error> {
        let range = if recovery {
            NORMAL_SLOTS..SLOTS
        } else {
            0..NORMAL_SLOTS
        };
        let slot = range
            .into_iter()
            .find(|&slot| self.pending[slot].is_none())
            .ok_or(Error::Busy)?;
        let sequence = self.next_sequence.ok_or(Error::Exhausted)?;
        let request = Request {
            session: self.session,
            sequence,
            slot,
            at,
            target,
            transition,
            recovery,
            cancelled,
        };
        let packet = request.encode()?;
        self.pending[slot] = Some(Pending {
            request,
            admission: None,
        });
        self.next_sequence = sequence.checked_add(1);
        Ok(packet)
    }
    /// Invalid, foreign, duplicate and out-of-order receipts do not release credit.
    pub fn receive(&mut self, bytes: &[u8]) -> Result<Response, Error> {
        let response = Response::decode(bytes)?;
        if response.session != self.session {
            return Err(Error::Session);
        }
        let pending = self.pending[response.slot].ok_or(Error::State)?;
        if response.sequence != pending.request.sequence || response.requested != pending.request.at
        {
            return Err(Error::Sequence);
        }
        match response.phase {
            Phase::Admission => {
                if pending.admission.is_some() {
                    return Err(Error::State);
                }
                if response.status == Status::Admitted {
                    self.pending[response.slot] = Some(Pending {
                        admission: Some(response),
                        ..pending
                    });
                } else {
                    self.pending[response.slot] = None;
                }
            }
            Phase::Execution => {
                let admission = pending.admission.ok_or(Error::State)?;
                if response.runtime_sequence != admission.runtime_sequence
                    || response.effective != admission.effective
                {
                    return Err(Error::Sequence);
                }
                self.pending[response.slot] = None;
            }
        }
        Ok(response)
    }
    pub fn outstanding(&self) -> usize {
        self.pending
            .iter()
            .filter(|pending| pending.is_some())
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn last_sequence_is_usable_once_without_wrapping() {
        let mut client = Client::new(7).unwrap();
        client.next_sequence = Some(u64::MAX);
        let target = LoopId::from_parts(0, 1).unwrap();
        let packet =
            crate::guarded(|| client.prepare(0, target, Transition::Stop, false, false)).unwrap();
        assert_eq!(Request::decode(&packet).unwrap().sequence, u64::MAX);
        assert_eq!(
            crate::guarded(|| client.prepare(0, target, Transition::Stop, false, false)),
            Err(Error::Exhausted)
        );
        assert_eq!(client.outstanding(), 1);
    }
}
