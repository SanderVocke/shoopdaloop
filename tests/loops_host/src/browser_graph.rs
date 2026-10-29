//! Public-binding checks for descriptor payloads, separate from the command bridge.
use crate::graph_transfer::{Id, Loan, Phase, Receiver, BANKS, RECORD_BYTES};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct BrowserGraphReceiver {
    receiver: Receiver,
    record: [u8; RECORD_BYTES],
    loans: [Option<Loan>; BANKS],
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl BrowserGraphReceiver {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new(session: u64) -> Result<Self, u32> {
        let config = rt_runtime::Config {
            max_nodes: 4,
            max_edges: 16,
            ..rt_runtime::Config::default()
        }
        .prepare()
        .map_err(crate::wire::runtime_error)?;
        Ok(Self {
            receiver: Receiver::new(session, config).map_err(crate::wire::runtime_error)?,
            record: [0; RECORD_BYTES],
            loans: std::array::from_fn(|_| None),
        })
    }
    pub fn record_ptr(&mut self) -> usize {
        self.record.as_mut_ptr() as usize
    }
    pub fn generation(&self, slot: u32) -> u32 {
        self.receiver
            .id(slot as usize)
            .map_or(0, |id| id.generation)
    }
    pub fn last_sequence(&self) -> u64 {
        self.receiver.last_sequence()
    }
    /// 0 idle, 1 receiving, 2 sealed, 3 loaned; high bit plus runtime error on refusal.
    pub fn phase(&self, slot: u32, generation: u32) -> u32 {
        match self.receiver.phase(Id { slot, generation }) {
            Ok(Phase::Idle) => 0,
            Ok(Phase::Receiving) => 1,
            Ok(Phase::Sealed) => 2,
            Ok(Phase::Loaned) => 3,
            Err(error) => 0x8000_0000 | crate::wire::runtime_error(error),
        }
    }
    pub fn ingest(&mut self) -> u32 {
        crate::guarded(|| self.receiver.ingest(&self.record))
            .map_or_else(crate::wire::runtime_error, |_| 0)
    }
    /// Local owner exchange only: this does not submit/admit a runtime graph.
    pub fn take(&mut self, slot: u32, generation: u32) -> u32 {
        crate::guarded(|| {
            let entry = self
                .loans
                .get_mut(slot as usize)
                .ok_or(rt_runtime::Error::InvalidId)?;
            if entry.is_some() {
                return Err(rt_runtime::Error::Busy);
            }
            *entry = Some(self.receiver.take(Id { slot, generation })?);
            Ok(())
        })
        .map_or_else(crate::wire::runtime_error, |_| 0)
    }
    pub fn loan_epoch(&self, slot: u32) -> u64 {
        self.loans
            .get(slot as usize)
            .and_then(Option::as_ref)
            .map_or(0, |loan| loan.epoch.0)
    }
    pub fn recycle(&mut self, slot: u32, generation: u32) -> u32 {
        crate::guarded(|| {
            let entry = self
                .loans
                .get_mut(slot as usize)
                .ok_or(rt_runtime::Error::InvalidId)?;
            if entry
                .as_ref()
                .is_none_or(|loan| loan.id != (Id { slot, generation }))
            {
                return Err(rt_runtime::Error::InvalidId);
            }
            let loan = entry.take().unwrap();
            match self.receiver.recycle(loan.id, loan.plan) {
                Ok(()) => Ok(()),
                Err((error, plan)) => {
                    *entry = Some(Loan {
                        id: loan.id,
                        epoch: loan.epoch,
                        plan,
                    });
                    Err(error)
                }
            }
        })
        .map_or_else(crate::wire::runtime_error, |_| 0)
    }
}
