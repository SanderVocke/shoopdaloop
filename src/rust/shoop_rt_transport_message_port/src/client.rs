//! Main-thread record batches exchanged through a MessagePort.
use shoop_rt_protocol::{codec, DisconnectReason, EngineMessage, Lane};
use shoop_rt_transport::carrier::{CarrierError, RecordPort};
use shoop_rt_transport::framing::{self, FramingError};

/// The client's record port over the processor's `MessagePort`. Posted records
/// collect into one outgoing batch; received batches are split into records.
///
/// When the worklet is gone, posting fails and the client reads
/// `Disconnected` after the records that had arrived.
#[derive(Debug, Default)]
pub struct RecordBatches {
    outgoing: Vec<u8>,
    incoming: Vec<u8>,
    read: usize,
    closed: Option<DisconnectReason>,
    close_delivered: bool,
    profiling_consumed: u32,
}

impl RecordBatches {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn take_profiling_credits(&mut self) -> u32 {
        std::mem::take(&mut self.profiling_consumed)
    }

    /// The records posted since the last call, as one batch.
    pub fn take_outgoing(&mut self) -> Option<Vec<u8>> {
        (!self.outgoing.is_empty()).then(|| std::mem::take(&mut self.outgoing))
    }

    /// Append a batch the processor posted.
    pub fn receive(&mut self, batch: &[u8]) {
        if self.closed.is_some() {
            return;
        }
        if self.read > 0 {
            self.incoming.drain(..self.read);
            self.read = 0;
        }
        self.incoming.extend_from_slice(batch);
    }

    /// End the connection from the carrier's side, for example because the
    /// processor failed or the audio graph closed.
    pub fn close(&mut self, reason: DisconnectReason) {
        if self.closed.is_none() {
            self.closed = Some(reason);
            self.outgoing.clear();
        }
    }

    pub fn is_closed(&self) -> bool {
        self.closed.is_some()
    }

    fn deliver_close(&mut self, buf: &mut [u8]) -> Option<usize> {
        let reason = self.closed?;
        if self.close_delivered {
            return None;
        }
        self.close_delivered = true;
        codec::encode_engine(&EngineMessage::Disconnected(reason), buf).ok()
    }
}

impl RecordPort for RecordBatches {
    fn consumed(&mut self, lane: Lane) {
        if lane == Lane::Profiling {
            self.profiling_consumed = self.profiling_consumed.saturating_add(1);
        }
    }
    fn post(&mut self, record: &[u8]) -> Result<(), CarrierError> {
        if self.closed.is_some() {
            return Err(CarrierError::Disconnected);
        }
        self.outgoing.extend_from_slice(record);
        Ok(())
    }

    fn take(&mut self, buf: &mut [u8]) -> Option<usize> {
        let pending = &self.incoming[self.read..];
        match framing::record_len(pending) {
            Ok(Some(len)) if len <= buf.len() => {
                buf[..len].copy_from_slice(&pending[..len]);
                self.read += len;
                if self.read == self.incoming.len() {
                    self.incoming.clear();
                    self.read = 0;
                }
                Some(len)
            }
            Ok(None) => self.deliver_close(buf),
            Ok(Some(_)) | Err(FramingError) => {
                self.incoming.clear();
                self.read = 0;
                self.close(DisconnectReason::ProtocolError);
                self.deliver_close(buf)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_rt_protocol::{codec, DisconnectReason, EngineMessage};
    use shoop_rt_protocol::{ClientMessage, MAX_RECORD_BYTES};
    use shoop_rt_transport::carrier::{Carrier, RecordClient};
    use shoop_rt_transport::framing;
    use shoop_wasm_test_support::shoop_test;

    fn engine_record(message: EngineMessage) -> Vec<u8> {
        let mut buf = [0; MAX_RECORD_BYTES];
        let len = codec::encode_engine(&message, &mut buf).unwrap();
        buf[..len].to_vec()
    }

    #[shoop_test]
    fn profiling_credit_returns_only_after_a_record_is_consumed() {
        let mut client = RecordClient::new(RecordBatches::new());
        let record = engine_record(EngineMessage::ProfilingStatus(Default::default()));
        client.port_mut().receive(&record);
        assert_eq!(client.port_mut().take_profiling_credits(), 0);
        assert!(matches!(
            client.receive(),
            Some(EngineMessage::ProfilingStatus(_))
        ));
        assert_eq!(client.port_mut().take_profiling_credits(), 1);
        assert_eq!(client.port_mut().take_profiling_credits(), 0);
        client.port_mut().receive(&record);
        client
            .port_mut()
            .receive(&engine_record(EngineMessage::Rejected { version: 99 }));
        assert!(client.receive().is_some());
        assert!(client.receive().is_some());
        assert_eq!(client.port_mut().take_profiling_credits(), 1);
    }

    #[shoop_test]
    fn posted_records_leave_as_one_batch() {
        let mut client = RecordClient::new(RecordBatches::new());
        assert_eq!(client.port_mut().take_outgoing(), None);
        client.send(ClientMessage::Hello { version: 1 }).unwrap();
        client.send(ClientMessage::Resync).unwrap();
        let batch = client.port_mut().take_outgoing().unwrap();
        let records = framing::records(&batch)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(
            codec::decode_client(records[0]).unwrap(),
            ClientMessage::Hello { version: 1 }
        );
        assert_eq!(
            codec::decode_client(records[1]).unwrap(),
            ClientMessage::Resync
        );
        assert_eq!(client.port_mut().take_outgoing(), None);
    }

    #[shoop_test]
    fn received_batches_split_into_records_across_posts() {
        let mut client = RecordClient::new(RecordBatches::new());
        let first = engine_record(EngineMessage::Disconnected(DisconnectReason::Shutdown));
        let second = engine_record(EngineMessage::Disconnected(DisconnectReason::Fault));
        let mut stream = first.clone();
        stream.extend_from_slice(&second);
        let (head, tail) = stream.split_at(first.len() + 1);
        client.port_mut().receive(head);
        assert_eq!(
            client.receive(),
            Some(EngineMessage::Disconnected(DisconnectReason::Shutdown))
        );
        assert_eq!(client.receive(), None);
        client.port_mut().receive(tail);
        assert_eq!(
            client.receive(),
            Some(EngineMessage::Disconnected(DisconnectReason::Fault))
        );
        assert_eq!(client.receive(), None);
    }

    #[shoop_test]
    fn closing_delivers_received_records_then_one_disconnect() {
        let mut client = RecordClient::new(RecordBatches::new());
        client.send(ClientMessage::Resync).unwrap();
        client
            .port_mut()
            .receive(&engine_record(EngineMessage::Disconnected(
                DisconnectReason::Shutdown,
            )));
        client.port_mut().close(DisconnectReason::Fault);
        assert!(client.port_mut().is_closed());
        assert_eq!(client.port_mut().take_outgoing(), None);
        assert_eq!(
            client.send(ClientMessage::Resync),
            Err(CarrierError::Disconnected)
        );
        assert_eq!(
            client.receive(),
            Some(EngineMessage::Disconnected(DisconnectReason::Shutdown))
        );
        assert_eq!(
            client.receive(),
            Some(EngineMessage::Disconnected(DisconnectReason::Fault))
        );
        assert_eq!(client.receive(), None);
        client.port_mut().receive(&[4, 0, 0, 0]);
        assert_eq!(client.receive(), None);
    }

    #[shoop_test]
    fn an_invalid_length_ends_the_connection_with_a_protocol_error() {
        let mut client = RecordClient::new(RecordBatches::new());
        client.port_mut().receive(&[1, 0, 9]);
        assert_eq!(
            client.receive(),
            Some(EngineMessage::Disconnected(DisconnectReason::ProtocolError))
        );
        assert_eq!(client.receive(), None);
        assert!(client.port_mut().is_closed());
    }
}
