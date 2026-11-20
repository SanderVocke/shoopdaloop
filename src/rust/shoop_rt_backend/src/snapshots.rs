//! Whole-snapshot assembly. No candidate state reaches the mirror before validation.
use crate::{EngineClock, Link, RtBackend};
use alloc::vec::Vec;
pub use shoop_rt_protocol::snapshot::SnapshotDelivery;
use shoop_rt_protocol::{
    snapshot::{Chunk, Header, Record, Records, HEADER},
    StateRecord, Telemetry,
};
use shoop_rt_transport::carrier::Carrier;

pub(crate) struct Snapshots {
    pub delivery: SnapshotDelivery,
    bytes: Vec<u8>,
    sequence: u32,
    accepted: u32,
    total: usize,
    received: usize,
    pub configuration: Option<(u64, u64)>,
    pub observed_configuration: Option<(u64, u64)>,
    loop_generations: Vec<u32>,
    strip_generations: Vec<u32>,
    bus_generations: Vec<u32>,
    pub frame: Option<u64>,
    pub after: u64,
    pub capture_after: u64,
    pub plans: alloc::collections::BTreeMap<
        shoop_backend::BackendLoopId,
        (shoop_backend::TransitionPlan, u32),
    >,
}
impl Default for Snapshots {
    fn default() -> Self {
        Self {
            delivery: Default::default(),
            bytes: Vec::new(),
            sequence: 0,
            accepted: 0,
            total: 0,
            received: 0,
            configuration: None,
            observed_configuration: None,
            loop_generations: Vec::new(),
            strip_generations: Vec::new(),
            bus_generations: Vec::new(),
            frame: None,
            after: 0,
            capture_after: 0,
            plans: Default::default(),
        }
    }
}
impl Snapshots {
    pub fn reset(&mut self) {
        let delivery = self.delivery;
        *self = Self::default();
        self.delivery = delivery;
    }
    pub fn capacity(&mut self, limits: shoop_rt_protocol::Limits) {
        if let Some(capacity) = shoop_rt_protocol::snapshot::capacity(
            limits.strips as usize,
            limits.loops as usize,
            limits.buses as usize,
        ) {
            self.bytes.resize(capacity, 0);
            self.loop_generations.resize(limits.loops as usize, 0);
            self.strip_generations.resize(limits.strips as usize, 0);
            self.bus_generations.resize(limits.buses as usize, 0);
        }
    }
    pub fn loop_address(&self, id: shoop_rt_protocol::LoopSlotId) -> u32 {
        address(&self.loop_generations, id.0)
    }
    pub fn strip_address(&self, id: shoop_rt_protocol::StripId) -> u32 {
        address(&self.strip_generations, id.0)
    }
    pub fn bus_address(&self, id: shoop_rt_protocol::BusId) -> u32 {
        address(&self.bus_generations, id.0)
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

fn address(generations: &[u32], slot: shoop_rt_protocol::SlotId) -> u32 {
    if slot.generation != 0
        && generations.get(slot.index as usize).copied() == Some(slot.generation)
    {
        slot.index
    } else {
        u32::MAX
    }
}

impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
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
        if !self.is_ready() {
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
    pub fn snapshot_frame(&self) -> Option<u64> {
        self.snapshots.frame
    }

    pub(crate) fn snapshot_chunk(&mut self, chunk: Chunk) {
        if self.link != Link::Ready {
            return;
        }
        let Some((header, mut records)) = self.snapshots.receive(chunk) else {
            return;
        };
        // Validate record classes and identities for the known configuration first.
        let known = |record: &Record| match record {
            Record::LoopPlan { id, .. }
            | Record::LoopLevels { id, .. }
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
        };
        if self.prepared_graph.is_some() {
            records.retain(known);
        } else if !records.iter().all(known) {
            return;
        }
        if self.prepared_graph.is_some() {
            self.snapshots
                .loop_generations
                .resize(self.mirror.loop_slots.capacity(), 0);
            self.snapshots
                .strip_generations
                .resize(self.mirror.strip_slots.capacity(), 0);
        }
        self.snapshots.loop_generations.fill(0);
        self.snapshots.strip_generations.fill(0);
        self.snapshots.bus_generations.fill(0);
        self.snapshots.plans.clear();
        for record in records {
            match record {
                Record::LoopPlan { id, plan, failures } => {
                    let mode = |m| match m {
                        shoop_rt_protocol::immediate::TransitionMode::Stopped => {
                            shoop_backend::TransitionMode::Stopped
                        }
                        shoop_rt_protocol::immediate::TransitionMode::Playing => {
                            shoop_backend::TransitionMode::Playing
                        }
                        shoop_rt_protocol::immediate::TransitionMode::Recording => {
                            shoop_backend::TransitionMode::Recording
                        }
                    };
                    let step = |s: shoop_rt_protocol::immediate::PlannedTransition| {
                        shoop_backend::PlannedTransition {
                            mode: mode(s.mode),
                            skip: s.skip,
                        }
                    };
                    if let Some(id) = self.mirror.slot_loops.get(&id) {
                        self.snapshots.plans.insert(
                            *id,
                            (
                                shoop_backend::TransitionPlan {
                                    now: None,
                                    first: plan.first.map(step),
                                    second: plan.second.map(step),
                                },
                                failures,
                            ),
                        );
                    }
                }
                Record::LoopLevels {
                    id,
                    gain,
                    balance,
                    repeat_sync,
                } => {
                    if let Some(owner) =
                        self.snapshots.loop_generations.get_mut(id.0.index as usize)
                    {
                        *owner = id.0.generation;
                    }
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
                    if let Some(owner) =
                        self.snapshots.bus_generations.get_mut(bus.0.index as usize)
                    {
                        *owner = bus.0.generation;
                    }
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
                Record::State(record) => {
                    if let StateRecord::StripControl { strip, .. } = record {
                        if let Some(owner) = self
                            .snapshots
                            .strip_generations
                            .get_mut(strip.0.index as usize)
                        {
                            *owner = strip.0.generation;
                        }
                    }
                    self.apply_state_record(record);
                }
            }
        }
        self.snapshots.accepted = chunk.sequence;
        self.snapshots.frame = Some(header.frame);
        if self.prepared_graph.is_some() {
            self.status.frame = header.frame;
            self.status.callbacks = header.frame / u64::from(self.limits().quantum.max(1));
        }
        self.snapshots.observed_configuration = Some((header.graph, header.topology));
        if header.frame > self.snapshots.after {
            self.telemetry_fresh = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;
    #[shoop_test]
    fn command_addresses_belong_to_the_observed_configuration() {
        use shoop_rt_protocol::{LoopSlotId, SlotId};
        let mut state = Snapshots::default();
        state.observed_configuration = Some((1, 2));
        state.loop_generations = alloc::vec![1];
        state.configuration = Some((2, 3));
        assert_eq!(state.observed_configuration, Some((1, 2)));
        assert_eq!(state.loop_address(LoopSlotId(SlotId::new(0, 1))), 0);
        assert_eq!(state.loop_address(LoopSlotId(SlotId::new(0, 2))), u32::MAX);
        assert_eq!(state.loop_address(LoopSlotId(SlotId::new(1, 1))), u32::MAX);
        state.loop_generations[0] = 2;
        state.observed_configuration = state.configuration;
        assert_eq!(state.loop_address(LoopSlotId(SlotId::new(0, 1))), u32::MAX);
        assert_eq!(state.loop_address(LoopSlotId(SlotId::new(0, 2))), 0);
    }
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
