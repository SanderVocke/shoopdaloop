//! Prepared record buffers at the worklet ABI boundary.
use shoop_rt_transport::buffer::{Batch, Stream};
use shoop_rt_transport::carrier::{CarrierError, RecordPort};
use shoop_rt_transport::framing;
use std::cell::RefCell;
use std::rc::Rc;

pub struct Records {
    pub inbox: Stream,
    pub outbox: Batch,
    pub framing_errors: u32,
}

/// The record lanes, shared by the record server, which owns a handle as its
/// port, and the host, which exposes their buffers to the script.
#[derive(Clone)]
pub struct RecordLanes(pub Rc<RefCell<Records>>);

impl RecordPort for RecordLanes {
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
