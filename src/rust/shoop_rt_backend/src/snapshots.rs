//! Whole-snapshot assembly. No candidate state reaches the mirror before validation.
use crate::{EngineClock, Link, RtBackend};
use alloc::vec::Vec;
pub use shoop_rt_protocol::snapshot::SnapshotDelivery;
use shoop_rt_protocol::{
    snapshot::{Chunk, Header, Record, Records, HEADER},
    StateRecord, Telemetry,
};
use shoop_rt_transport::carrier::Carrier;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Observation {
    Legacy,
    #[default]
    Snapshots,
}

pub(crate) struct Snapshots {
    pub mode: Observation,
    pub delivery: SnapshotDelivery,
    bytes: Vec<u8>,
    sequence: u32,
    accepted: u32,
    total: usize,
    received: usize,
    pub configuration: Option<(u64, u64)>,
    pub frame: Option<u64>,
    pub after: u64,
}
impl Default for Snapshots {
    fn default() -> Self {
        Self {
            mode: Observation::Snapshots,
            delivery: Default::default(),
            bytes: Vec::new(),
            sequence: 0,
            accepted: 0,
            total: 0,
            received: 0,
            configuration: None,
            frame: None,
            after: 0,
        }
    }
}
impl Snapshots {
    pub fn reset(&mut self) {
        let mode = self.mode;
        let delivery = self.delivery;
        *self = Self::default();
        self.mode = mode;
        self.delivery = delivery;
    }
    pub fn capacity(&mut self, limits: shoop_rt_protocol::Limits) {
        if let Some(capacity) = shoop_rt_protocol::snapshot::capacity(
            limits.strips as usize,
            limits.loops as usize,
            limits.buses as usize,
        ) {
            self.bytes.resize(capacity, 0);
        }
    }
    fn receive(&mut self, chunk: Chunk) -> Option<(Header, Vec<Record>)> {
        let total = chunk.total as usize;
        let offset = chunk.offset as usize;
        let len = usize::from(chunk.len);
        if chunk.sequence <= self.accepted
            || total < HEADER
            || total > self.bytes.len()
            || len > chunk.bytes.len()
            || offset.checked_add(len)? > total
        {
            return None;
        }
        if chunk.offset == 0 && chunk.sequence >= self.sequence {
            self.sequence = chunk.sequence;
            self.total = total;
            self.received = 0;
        }
        if chunk.sequence != self.sequence || total != self.total || offset != self.received {
            return None;
        }
        self.bytes[offset..offset + len].copy_from_slice(&chunk.bytes[..len]);
        self.received += len;
        if self.received != self.total {
            return None;
        }
        let header = Header::read(&self.bytes[..total])?;
        if self.configuration != Some((header.graph, header.topology))
            || self.frame.is_some_and(|frame| header.frame < frame)
        {
            return None;
        }
        let records = Records(&self.bytes[HEADER..total])
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        if records.len() != header.records as usize {
            return None;
        }
        let loops = records
            .iter()
            .filter(|r| matches!(r, Record::LoopLevels { .. }))
            .count();
        let strips = records
            .iter()
            .filter(|r| matches!(r, Record::Telemetry(Telemetry::Strip { .. })))
            .count();
        let buses = records
            .iter()
            .filter(|r| matches!(r, Record::Telemetry(Telemetry::Bus { .. })))
            .count();
        if (loops, strips, buses)
            != (
                header.loops as usize,
                header.strips as usize,
                header.buses as usize,
            )
        {
            return None;
        }
        Some((header, records))
    }
}

impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    /// Select one authoritative observation source; legacy results still drain.
    pub fn set_observation(&mut self, mode: Observation) -> anyhow::Result<()> {
        self.client
            .observe_snapshots(mode == Observation::Snapshots)
            .map_err(|error| anyhow::anyhow!("could not select observation: {error:?}"))?;
        self.snapshots.mode = mode;
        Ok(())
    }
    /// Push is rate-limited; pull serves explicit requests without waiting for RT.
    pub fn set_snapshot_delivery(&mut self, delivery: SnapshotDelivery) -> anyhow::Result<()> {
        self.client
            .configure_snapshots(delivery)
            .map_err(|error| anyhow::anyhow!("could not select snapshot delivery: {error:?}"))?;
        self.snapshots.delivery = delivery;
        Ok(())
    }
    pub fn snapshot_delivery(&self) -> SnapshotDelivery {
        self.snapshots.delivery
    }
    /// Queue at most one request. Drawing can continue with the last accepted state.
    pub fn request_latest_snapshot(&mut self) -> anyhow::Result<bool> {
        if !self.is_ready() || self.snapshots.mode != Observation::Snapshots {
            return Ok(false);
        }
        match self.client.request_snapshot(self.snapshots.accepted) {
            Ok(()) => Ok(true),
            Err(shoop_rt_transport::client::ClientError::Busy)
            | Err(shoop_rt_transport::client::ClientError::Carrier(
                shoop_rt_transport::carrier::CarrierError::Full,
            )) => Ok(false),
            Err(error) => Err(anyhow::anyhow!("could not request snapshot: {error:?}")),
        }
    }
    pub fn observation(&self) -> Observation {
        self.snapshots.mode
    }
    pub fn snapshot_frame(&self) -> Option<u64> {
        self.snapshots.frame
    }

    pub(crate) fn snapshot_chunk(&mut self, chunk: Chunk) {
        if self.snapshots.mode != Observation::Snapshots || self.link != Link::Ready {
            return;
        }
        let Some((header, records)) = self.snapshots.receive(chunk) else {
            return;
        };
        // Validate record classes and identities for the known configuration first.
        if !records.iter().all(|record| match record {
            Record::LoopLevels { id, .. }
            | Record::State(StateRecord::LoopTiming { loop_id: id, .. })
            | Record::Telemetry(Telemetry::Loop { loop_id: id, .. }) => {
                self.mirror.slot_loops.contains_key(id)
            }
            Record::State(StateRecord::StripControl { strip, .. })
            | Record::Telemetry(Telemetry::Strip { strip, .. }) => {
                self.mirror.strip_tracks.contains_key(strip)
            }
            Record::State(StateRecord::Bus { bus, .. })
            | Record::Telemetry(Telemetry::Bus { bus, .. }) => {
                self.mirror.buses.values().any(|entry| entry.slot == *bus)
            }
            _ => false,
        }) {
            return;
        }
        for record in records {
            match record {
                Record::LoopLevels {
                    id,
                    gain,
                    balance,
                    repeat_sync,
                } => {
                    if let Some(id) = self.mirror.slot_loops.get(&id) {
                        if let Some(entry) = self.mirror.loops.get_mut(id) {
                            entry.gain = gain;
                            entry.balance = balance;
                            entry.repeat_sync = repeat_sync;
                        }
                    }
                }
                Record::Telemetry(record) => self.telemetry(record),
                Record::State(StateRecord::Bus { bus, control }) => {
                    use shoop_backend::BackendBusControl;
                    use shoop_rt_protocol::BusControl;
                    if let Some(entry) = self
                        .mirror
                        .buses
                        .values_mut()
                        .find(|entry| entry.slot == bus)
                    {
                        entry.apply(match control {
                            BusControl::Gain(value) => BackendBusControl::GainDb(
                                crate::mirror::reconciled_db(entry.gain_db, value),
                            ),
                            BusControl::Balance(value) => BackendBusControl::Balance(value),
                            BusControl::Mute(value) => BackendBusControl::Mute(value),
                        });
                    }
                }
                Record::State(record) => self.apply_state_record(record),
            }
        }
        self.snapshots.accepted = chunk.sequence;
        self.snapshots.frame = Some(header.frame);
        if header.frame > self.snapshots.after {
            self.telemetry_fresh = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;
    fn chunk(bytes: &[u8], sequence: u32, offset: usize, len: usize) -> Chunk {
        let mut chunk = Chunk {
            sequence,
            total: bytes.len() as u32,
            offset: offset as u32,
            len: len as u16,
            bytes: [0; shoop_rt_protocol::snapshot::CHUNK_BYTES],
        };
        chunk.bytes[..len].copy_from_slice(&bytes[offset..offset + len]);
        chunk
    }
    #[shoop_test]
    fn partial_stale_unknown_and_malformed_publications_do_not_commit() {
        let mut state = Snapshots::default();
        state.bytes.resize(4096, 0);
        state.configuration = Some((1, 2));
        let mut bytes = [0; HEADER];
        Header {
            frame: 100,
            graph: 1,
            topology: 2,
            records: 0,
            loops: 0,
            strips: 0,
            buses: 0,
        }
        .write(&mut bytes);
        assert!(state.receive(chunk(&bytes, 2, 0, 20)).is_none());
        assert_eq!(state.accepted, 0);
        assert!(state.receive(chunk(&bytes, 2, 20, HEADER - 20)).is_some());
        state.accepted = 2;
        assert!(state.receive(chunk(&bytes, 2, 0, HEADER)).is_none());
        assert!(state.receive(chunk(&bytes, 4, 20, HEADER - 20)).is_none());
        state.configuration = Some((2, 3));
        assert!(state.receive(chunk(&bytes, 4, 0, HEADER)).is_none());
        state.configuration = Some((1, 2));
        bytes[28..32].copy_from_slice(&1u32.to_le_bytes());
        assert!(state.receive(chunk(&bytes, 6, 0, HEADER)).is_none());
        assert_eq!(state.accepted, 2);
    }
}
