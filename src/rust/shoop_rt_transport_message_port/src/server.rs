//! Prepared record buffers at the worklet ABI boundary.
use shoop_rt_protocol::Lane;
use shoop_rt_transport::buffer::{Batch, Stream};
use shoop_rt_transport::carrier::{CarrierError, RecordPort};
use shoop_rt_transport::framing;
use std::cell::RefCell;
use std::rc::Rc;

pub const PROFILING_CREDITS: usize = 256;

pub struct Records {
    pub inbox: Stream,
    pub outbox: Batch,
    pub framing_errors: u32,
    pub profiling_credits: usize,
}

/// The record lanes, shared by the record server, which owns a handle as its
/// port, and the host, which exposes their buffers to the script.
#[derive(Clone)]
pub struct RecordLanes(pub Rc<RefCell<Records>>);

impl RecordLanes {
    pub fn grant_profiling(&self, count: u32) {
        let mut records = self.0.borrow_mut();
        records.profiling_credits = records
            .profiling_credits
            .saturating_add(count as usize)
            .min(PROFILING_CREDITS);
    }
}

impl RecordPort for RecordLanes {
    fn post_on_lane(&mut self, record: &[u8], lane: Lane) -> Result<(), CarrierError> {
        let mut records = self.0.borrow_mut();
        if lane == Lane::Profiling && records.profiling_credits == 0 {
            return Err(CarrierError::Full);
        }
        if !records.outbox.push(record) {
            return Err(CarrierError::Full);
        }
        if lane == Lane::Profiling {
            records.profiling_credits -= 1;
        }
        Ok(())
    }
    fn post(&mut self, record: &[u8]) -> Result<(), CarrierError> {
        if self.0.borrow_mut().outbox.push(record) {
            Ok(())
        } else {
            Err(CarrierError::Full)
        }
    }

    fn take(&mut self, buf: &mut [u8]) -> Option<usize> {
        let mut records = self.0.borrow_mut();
        let pending = records.inbox.pending();
        match framing::record_len(pending) {
            Ok(None) => None,
            Ok(Some(len)) if len <= buf.len() => {
                buf[..len].copy_from_slice(&pending[..len]);
                records.inbox.consume(len);
                Some(len)
            }
            _ => {
                // The stream cannot be split any further. The invalid header makes
                // the server end the connection with a protocol error.
                buf[..2].copy_from_slice(&pending[..2]);
                records.inbox.clear();
                records.framing_errors = records.framing_errors.saturating_add(1);
                Some(2)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test]
    fn stalled_profiling_cannot_consume_other_lanes_or_accumulate_credits() {
        let mut port = RecordLanes(Rc::new(RefCell::new(Records {
            inbox: Stream::new(128),
            outbox: Batch::new(128),
            framing_errors: 0,
            profiling_credits: 1,
        })));
        let record = [3, 0, 8];
        assert_eq!(port.post_on_lane(&record, Lane::Profiling), Ok(()));
        port.0.borrow_mut().outbox.clear();
        for _ in 0..1024 {
            assert_eq!(
                port.post_on_lane(&record, Lane::Profiling),
                Err(CarrierError::Full)
            );
            assert_eq!(port.post_on_lane(&record, Lane::Reliable), Ok(()));
            port.0.borrow_mut().outbox.clear();
        }
        port.grant_profiling(u32::MAX);
        assert_eq!(port.0.borrow().profiling_credits, PROFILING_CREDITS);
        for _ in 0..PROFILING_CREDITS {
            assert_eq!(port.post_on_lane(&record, Lane::Profiling), Ok(()));
            port.0.borrow_mut().outbox.clear();
        }
        assert_eq!(
            port.post_on_lane(&record, Lane::Profiling),
            Err(CarrierError::Full)
        );
    }
}
