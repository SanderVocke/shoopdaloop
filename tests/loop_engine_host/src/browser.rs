//! Prepared Wasm-facing buffers. Pointer exports address only this instance's memory.
use crate::{allocation_guard, wire, Bridge, Client};
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    Frame,
};
use shoop_loop_engine::{LoopId, Transition};
use std::sync::atomic::{AtomicU32, Ordering};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

pub const HEAP_FAILURE: u32 = 0xffff_0001;
const JOURNAL_BYTES: usize = 16_384;
static MARKER: AtomicU32 = AtomicU32::new(0);
pub(crate) fn protocol_error(error: wire::Error) -> u32 {
    use wire::Error::*;
    match error {
        Size => 1,
        Format => 2,
        Version => 3,
        Reserved => 4,
        Operation => 5,
        Identity => 6,
        Slot => 7,
        Sequence => 8,
        Session => 9,
        Busy => 10,
        State => 11,
        Exhausted => 12,
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct BrowserClient {
    client: Client,
    request: [u8; wire::REQUEST_BYTES],
    response: [u8; wire::RESPONSE_BYTES],
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl BrowserClient {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new(session: u64) -> Result<Self, u32> {
        Ok(Self {
            client: Client::new(session).map_err(protocol_error)?,
            request: [0; wire::REQUEST_BYTES],
            response: [0; wire::RESPONSE_BYTES],
        })
    }
    pub fn prepare(&mut self, kind: u32, at: u64, position: u64, flags: u32) -> u32 {
        if flags & !7 != 0 {
            return 4;
        }
        let option = flags & 1 != 0;
        let transition = match kind {
            1 if position == 0 => Transition::Record {
                monitor_previous: option,
            },
            2 => Transition::Play {
                position,
                retrigger: option,
            },
            3 if position == 0 && !option => Transition::Stop,
            4 => Transition::Seek {
                position,
                retrigger: option,
            },
            _ => return 5,
        };
        match crate::guarded(|| {
            self.client.prepare(
                at,
                LoopId::from_parts(0, 1).unwrap(),
                transition,
                flags & 4 != 0,
                flags & 2 != 0,
            )
        }) {
            Ok(packet) => {
                self.request = packet;
                0
            }
            Err(error) => protocol_error(error),
        }
    }
    pub fn receive(&mut self) -> u32 {
        crate::guarded(|| self.client.receive(&self.response)).map_or_else(protocol_error, |_| 0)
    }
    pub fn outstanding(&self) -> u32 {
        self.client.outstanding() as u32
    }
    pub fn request_ptr(&self) -> usize {
        self.request.as_ptr() as usize
    }
    pub fn response_ptr(&mut self) -> usize {
        self.response.as_mut_ptr() as usize
    }
    pub fn set_marker(&self, value: u32) {
        MARKER.store(value, Ordering::Relaxed);
    }
    pub fn marker(&self) -> u32 {
        MARKER.load(Ordering::Relaxed)
    }
}

struct GraphBuffers {
    payload: [u8; crate::graph_transfer::RECORD_BYTES],
    request: [u8; crate::graph_wire::REQUEST_BYTES],
    response: [u8; crate::graph_wire::RESPONSE_BYTES],
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct BrowserBridge {
    core: Option<Bridge>,
    graph: Option<Box<GraphBuffers>>,
    request: [u8; wire::REQUEST_BYTES],
    response: [u8; wire::RESPONSE_BYTES],
    input_left: [f32; 128],
    input_right: [f32; 128],
    left: [f32; 128],
    right: [f32; 128],
    midi_input: [MidiBuffer; 1],
    midi_output: [MidiBuffer; 1],
    journal: [u8; JOURNAL_BYTES],
    journal_len: usize,
    journal_overflow: u32,
    semantic: Box<[u8]>,
    semantic_len: usize,
    heap: allocation_guard::Counts,
    frame: u64,
    callbacks: u64,
    faulted: bool,
    recovered: [u32; 4],
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl BrowserBridge {
    /// Fault modes: 0 normal, 1 actual FX processor error, 2 actual FX heap violation.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new(session: u64, fault: u8) -> Result<Self, u32> {
        Ok(Self::from_core(
            Bridge::with_fault(session, fault).map_err(protocol_error)?,
            false,
        ))
    }
    /// Opt-in fixed graph descriptor and command/result banks, prepared off-render.
    pub fn new_graphs(session: u64, fault: u8) -> Result<Self, u32> {
        Ok(Self::from_core(
            Bridge::with_graphs_fault(session, fault).map_err(protocol_error)?,
            true,
        ))
    }
}
impl BrowserBridge {
    fn from_core(core: Bridge, graphs: bool) -> Self {
        Self {
            core: Some(core),
            graph: graphs.then(|| {
                Box::new(GraphBuffers {
                    payload: [0; crate::graph_transfer::RECORD_BYTES],
                    request: [0; crate::graph_wire::REQUEST_BYTES],
                    response: [0; crate::graph_wire::RESPONSE_BYTES],
                })
            }),
            request: [0; wire::REQUEST_BYTES],
            response: [0; wire::RESPONSE_BYTES],
            input_left: [0.; 128],
            input_right: [0.; 128],
            left: [0.; 128],
            right: [0.; 128],
            midi_input: [MidiBuffer::new(2, 6).unwrap()],
            midi_output: [MidiBuffer::new(256, 1024).unwrap()],
            journal: [0; JOURNAL_BYTES],
            journal_len: 0,
            journal_overflow: 0,
            semantic: vec![0; crate::journal::BYTES].into_boxed_slice(),
            semantic_len: 0,
            heap: allocation_guard::Counts::default(),
            frame: 0,
            callbacks: 0,
            faulted: false,
            recovered: [0; 4],
        }
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl BrowserBridge {
    pub fn graph_payload_ptr(&mut self) -> usize {
        self.graph
            .as_mut()
            .map_or(0, |graph| graph.payload.as_mut_ptr() as usize)
    }
    pub fn graph_request_ptr(&mut self) -> usize {
        self.graph
            .as_mut()
            .map_or(0, |graph| graph.request.as_mut_ptr() as usize)
    }
    pub fn graph_response_ptr(&self) -> usize {
        self.graph
            .as_ref()
            .map_or(0, |graph| graph.response.as_ptr() as usize)
    }
    pub fn retained_graphs(&self) -> u32 {
        self.core
            .as_ref()
            .map_or(0, |core| core.retained_graphs() as u32)
    }
    pub fn ingest_graph_payload(&mut self) -> u32 {
        if self.faulted {
            return 10;
        }
        let (Some(core), Some(graph)) = (&mut self.core, &self.graph) else {
            return 10;
        };
        core.graph_payload(&graph.payload)
            .map_or_else(wire::runtime_error, |_| 0)
    }
    pub fn ingest_graph(&mut self) -> u32 {
        if self.faulted {
            return 11;
        }
        let (Some(core), Some(graph)) = (&mut self.core, &self.graph) else {
            return 11;
        };
        core.graph_ingest(&graph.request)
            .map_or_else(protocol_error, |_| 0)
    }
    /// Same polling convention as primitive responses; this bank is128 bytes.
    pub fn poll_graph_response(&mut self) -> u32 {
        let (Some(core), Some(graph)) = (&mut self.core, &mut self.graph) else {
            return 0x8000_000b;
        };
        match core.next_graph_response() {
            Ok(Some(packet)) => {
                graph.response = packet;
                1
            }
            Ok(None) => 0,
            Err(error) => 0x8000_0000 | protocol_error(error),
        }
    }
    pub fn request_ptr(&mut self) -> usize {
        self.request.as_mut_ptr() as usize
    }
    pub fn response_ptr(&self) -> usize {
        self.response.as_ptr() as usize
    }
    pub fn output_ptr(&self, channel: u32) -> usize {
        match channel {
            0 => self.left.as_ptr() as usize,
            1 => self.right.as_ptr() as usize,
            _ => 0,
        }
    }
    pub fn journal_ptr(&self) -> usize {
        self.journal.as_ptr() as usize
    }
    pub fn journal_len(&self) -> u32 {
        self.journal_len as u32
    }
    pub fn journal_overflow(&self) -> u32 {
        self.journal_overflow
    }
    pub fn semantic_ptr(&self) -> usize {
        self.semantic.as_ptr() as usize
    }
    pub fn semantic_len(&self) -> u32 {
        self.semantic_len as u32
    }
    pub fn marker(&self) -> u32 {
        MARKER.load(Ordering::Relaxed)
    }
    pub fn frame(&self) -> u64 {
        self.frame
    }
    pub fn callbacks(&self) -> u64 {
        self.callbacks
    }
    pub fn fault_witness(&self) -> u32 {
        self.core.as_ref().map_or(0, Bridge::fault_witness)
    }
    pub fn heap_allocations(&self) -> u32 {
        self.heap.allocations.min(u32::MAX as usize) as u32
    }
    pub fn heap_reallocations(&self) -> u32 {
        self.heap.reallocations.min(u32::MAX as usize) as u32
    }
    pub fn heap_deallocations(&self) -> u32 {
        self.heap.deallocations.min(u32::MAX as usize) as u32
    }
    pub fn recovered_pages(&self) -> u32 {
        self.recovered[0]
    }
    pub fn recovered_versions(&self) -> u32 {
        self.recovered[1]
    }
    pub fn recovered_snapshots(&self) -> u32 {
        self.recovered[2]
    }
    pub fn media_references(&self) -> u32 {
        self.recovered[3]
    }
    pub fn outstanding(&self) -> u32 {
        self.core
            .as_ref()
            .map_or(0, |core| core.outstanding() as u32)
    }
    pub fn ingest(&mut self) -> u32 {
        if self.faulted {
            return 11;
        }
        self.core
            .as_mut()
            .ok_or(wire::Error::State)
            .and_then(|core| core.ingest(&self.request))
            .map_or_else(protocol_error, |_| 0)
    }
    /// 0: no response, 1: fixed response ready, high bit: protocol error.
    pub fn poll_response(&mut self) -> u32 {
        match self
            .core
            .as_mut()
            .ok_or(wire::Error::State)
            .and_then(Bridge::next_response)
        {
            Ok(Some(packet)) => {
                self.response = packet;
                1
            }
            Ok(None) => 0,
            Err(error) => 0x8000_0000 | protocol_error(error),
        }
    }
    /// Covers source filling, actual rendering, receipt collection and MIDI logging.
    /// Heap violations are explicit fatal results with independent count witnesses.
    pub fn render(&mut self, frames: u32) -> u32 {
        if self.faulted || self.core.is_none() {
            return 10;
        }
        if frames > 128 {
            self.faulted = true;
            return 4;
        }
        let (result, counts) = allocation_guard::measure(|| self.render_inner(frames as usize));
        self.heap = counts;
        if counts != allocation_guard::Counts::default() {
            self.faulted = true;
            return HEAP_FAILURE;
        }
        if result != 0 {
            self.faulted = true;
        }
        result
    }
    /// Call only after this host has stopped invoking render. Pending results are
    /// allowed only on the explicit fault/abandonment path, never reported as success.
    pub fn stop(&mut self) -> u32 {
        let Some(core) = self.core.as_ref() else {
            return 10;
        };
        if (core.outstanding() != 0 || core.retained_graphs() != 0) && !self.faulted {
            return 15;
        }
        let mut stopped = self.core.take().unwrap().into_stopped();
        let semantic = stopped
            .observer()
            .as_any()
            .downcast_ref::<crate::journal::Journal>()
            .unwrap()
            .bytes();
        self.semantic[..semantic.len()].copy_from_slice(semantic);
        self.semantic_len = semantic.len();
        if let Err(error) = stopped.cleanup() {
            return wire::runtime_error(error);
        }
        for _ in 0..512 {
            stopped.resources().begin_callback();
            stopped.resources().reclaim();
        }
        let resources = stopped.resource_usage();
        let media = stopped.media_usage();
        self.recovered = [
            resources.free_pages as u32,
            resources.free_versions as u32,
            resources.free_snapshots as u32,
            media.references as u32,
        ];
        0
    }
}
impl BrowserBridge {
    fn render_inner(&mut self, frames: usize) -> u32 {
        let core = self.core.as_mut().unwrap();
        let start = core.next_frame();
        self.left.fill(0.);
        self.right.fill(0.);
        self.input_left.fill(0.);
        self.input_right.fill(0.);
        self.midi_input[0].clear();
        self.midi_output[0].clear();
        for offset in 0..frames {
            let Some(at) = start.checked_add(offset as u64) else {
                return 3;
            };
            if at < 4 {
                self.input_left[offset] = 2. * (at + 1) as f32;
                self.input_right[offset] = self.input_left[offset] * 10.;
            }
        }
        for (at, packet) in [(0, [0x90, 60, 100]), (2, [0x80, 60, 45])] {
            if at >= start && (at - start) < frames as u64 {
                if let Err(error) = self.midi_input[0].push((at - start) as u32, frames, &packet) {
                    return wire::runtime_error(error);
                }
            }
        }
        let result = core.process(
            AudioBlock {
                frame: Frame(start),
                frames,
                inputs: &[&self.input_left[..frames], &self.input_right[..frames]],
                outputs: &mut [&mut self.left[..frames], &mut self.right[..frames]],
            },
            MidiBlock {
                inputs: &self.midi_input,
                outputs: &mut self.midi_output,
            },
        );
        self.frame = core.next_frame();
        if frames != 0 {
            self.callbacks = self.callbacks.saturating_add(1);
        }
        if let Err(error) = result {
            return wire::runtime_error(error);
        }
        for index in 0..self.midi_output[0].len() {
            let (offset, packet) = self.midi_output[0].get(index).unwrap();
            let bytes = 12 + packet.len();
            if self.journal_len + bytes > self.journal.len() {
                self.journal_overflow = self.journal_overflow.saturating_add(1);
                continue;
            }
            let entry = &mut self.journal[self.journal_len..self.journal_len + bytes];
            entry[..8].copy_from_slice(&(start + u64::from(offset)).to_le_bytes());
            entry[8..12].copy_from_slice(&(packet.len() as u32).to_le_bytes());
            entry[12..].copy_from_slice(packet);
            self.journal_len += bytes;
        }
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn submit(client: &mut BrowserClient, bridge: &mut BrowserBridge) {
        for (kind, at) in [(1, 0), (2, 4), (3, 12)] {
            assert_eq!(client.prepare(kind, at, 0, 0), 0);
            bridge.request.copy_from_slice(&client.request);
            assert_eq!(bridge.ingest(), 0);
        }
    }
    fn collect(client: &mut BrowserClient, bridge: &mut BrowserBridge) {
        while bridge.poll_response() == 1 {
            client.response.copy_from_slice(&bridge.response);
            assert_eq!(client.receive(), 0);
        }
    }
    #[test]
    fn prepared_binding_buffers_match_native_media_and_recover_all_owners() {
        let expected = crate::run(&[13]);
        let mut client = BrowserClient::new(u64::MAX).unwrap();
        let mut bridge = BrowserBridge::new(u64::MAX, 0).unwrap();
        submit(&mut client, &mut bridge);
        assert_eq!(bridge.render(0), 0);
        assert_eq!(bridge.frame(), 0);
        assert_eq!(bridge.callbacks(), 0);
        assert_eq!(bridge.render(128), 0);
        for (at, sample) in expected.samples.iter().enumerate() {
            assert_eq!([bridge.left[at], bridge.right[at]], *sample);
        }
        assert!(bridge.left[13..]
            .iter()
            .chain(&bridge.right[13..])
            .all(|&sample| sample == 0.));
        let mut cursor = 0;
        for (frame, packet) in expected.midi {
            assert_eq!(
                u64::from_le_bytes(bridge.journal[cursor..cursor + 8].try_into().unwrap()),
                frame
            );
            assert_eq!(
                u32::from_le_bytes(bridge.journal[cursor + 8..cursor + 12].try_into().unwrap())
                    as usize,
                packet.len()
            );
            assert_eq!(
                &bridge.journal[cursor + 12..cursor + 12 + packet.len()],
                packet
            );
            cursor += 12 + packet.len();
        }
        assert_eq!(cursor, bridge.journal_len);
        assert_eq!(bridge.stop(), 15);
        collect(&mut client, &mut bridge);
        assert_eq!(client.outstanding(), 0);
        assert_eq!(bridge.render(128), 0);
        assert_eq!(bridge.journal_len, cursor);
        assert_eq!(bridge.journal_overflow(), 0);
        assert_eq!(bridge.stop(), 0);
        assert_eq!(bridge.recovered, [256, 16, 16, 0]);
        assert_eq!(&bridge.semantic[..bridge.semantic_len], expected.semantic);
        assert_eq!(bridge.render(128), 10);
    }
    #[test]
    fn actual_processor_and_heap_failures_have_distinct_witnesses() {
        for mode in [1, 2] {
            let mut client = BrowserClient::new(41).unwrap();
            let mut bridge = BrowserBridge::new(41, mode).unwrap();
            submit(&mut client, &mut bridge);
            assert_eq!(
                bridge.render(128),
                if mode == 1 { 10 } else { HEAP_FAILURE }
            );
            assert_eq!(bridge.fault_witness(), u32::from(mode));
            if mode == 1 {
                assert_eq!(bridge.heap, allocation_guard::Counts::default());
            } else {
                assert!(
                    bridge.heap_allocations() > 0
                        && bridge.heap_reallocations() > 0
                        && bridge.heap_deallocations() > 0
                );
            }
            assert_eq!(bridge.render(128), 10);
            assert_eq!(bridge.stop(), 0);
            assert_eq!(bridge.recovered, [256, 16, 16, 0]);
        }
    }
    #[test]
    fn same_position_seek_is_observed_without_inventing_a_transition_or_pulse() {
        let mut client = BrowserClient::new(45).unwrap();
        let mut bridge = BrowserBridge::new(45, 0).unwrap();
        submit(&mut client, &mut bridge);
        assert_eq!(client.prepare(4, 5, 1, 0), 0);
        bridge.request.copy_from_slice(&client.request);
        assert_eq!(bridge.ingest(), 0);
        assert_eq!(bridge.render(128), 0);
        assert_eq!(
            &bridge.left[..13],
            &[1., 2., 3., 4., 0.5, 1., 1.5, 2., 0.5, 1., 1.5, 2., 0.]
        );
        collect(&mut client, &mut bridge);
        assert_eq!(bridge.stop(), 0);
        let bytes = &bridge.semantic[..bridge.semantic_len];
        assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 17);
        let at = crate::journal::HEADER + 8 * crate::journal::STRIDE;
        let boundary = &bytes[at..at + crate::journal::STRIDE];
        assert_eq!(boundary[0], 1);
        assert_eq!(u64::from_le_bytes(boundary[8..16].try_into().unwrap()), 5);
        assert_eq!(u32::from_le_bytes(boundary[20..24].try_into().unwrap()), 0);
        let decision = &bytes[at + crate::journal::STRIDE..at + 2 * crate::journal::STRIDE];
        assert_eq!(decision[0], 2);
        assert_eq!(decision[2], 73);
        assert_eq!(u64::from_le_bytes(decision[40..48].try_into().unwrap()), 1);
    }

    fn graph_submit(client: &crate::BrowserGraphClient, bridge: &mut BrowserBridge) {
        bridge.graph.as_mut().unwrap().request = client.request;
        assert_eq!(bridge.ingest_graph(), 0);
    }
    fn graph_collect(
        client: &mut crate::BrowserGraphClient,
        bridge: &mut BrowserBridge,
    ) -> Vec<crate::graph_wire::Response> {
        let mut responses = Vec::new();
        loop {
            let result = bridge.poll_graph_response();
            if result == 0 {
                break;
            }
            assert_eq!(result, 1);
            client.response = bridge.graph.as_ref().unwrap().response;
            assert_eq!(client.receive(), 0);
            responses.push(crate::graph_wire::Response::decode(&client.response).unwrap());
        }
        responses
    }
    fn graph_upload(client: &mut crate::BrowserGraphClient, bridge: &mut BrowserBridge) {
        for index in 0..client.payload_count() {
            assert_eq!(
                client.prepare_payload(index, 0, 1, 1, u64::from(index) + 1),
                0
            );
            bridge.graph.as_mut().unwrap().payload = client.payload;
            assert_eq!(bridge.ingest_graph_payload(), 0);
        }
        assert_eq!(client.stage(0, 1, 0, 1, false), 0);
        graph_submit(client, bridge);
    }
    #[test]
    fn graph_binding_buffers_cover_route_cut_and_reliable_owning_release() {
        use crate::graph_wire::Status;
        let expected = crate::run(&[13]);
        for remainder in [
            vec![12],
            vec![3, 8, 1],
            vec![0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0],
        ] {
            let mut client = BrowserClient::new(u64::MAX).unwrap();
            let mut graph = crate::BrowserGraphClient::new(u64::MAX).unwrap();
            let mut bridge = BrowserBridge::new_graphs(u64::MAX, 0).unwrap();
            submit(&mut client, &mut bridge);
            graph_upload(&mut graph, &mut bridge);
            assert_eq!(bridge.render(0), 0);
            assert_eq!(bridge.frame(), 0);
            assert_eq!(bridge.render(1), 0);
            let mut samples = vec![[bridge.left[0], bridge.right[0]]];
            let staged = graph_collect(&mut graph, &mut bridge);
            assert_eq!(staged.len(), 1);
            assert_eq!(staged[0].status, Status::Staged);
            assert_eq!(graph.retained_graphs(), 1);
            let [scope, slot, generation] = staged[0].graph.unwrap().parts();
            assert_eq!(graph.activate(scope, slot, generation, 4, 1, 0, false), 0);
            graph_submit(&graph, &mut bridge);
            for frames in remainder {
                assert_eq!(bridge.render(frames), 0);
                samples.extend((0..frames as usize).map(|at| [bridge.left[at], bridge.right[at]]));
            }
            for (at, &sample) in samples.iter().enumerate() {
                let factor = if (4..12).contains(&at) { 2. } else { 1. };
                assert_eq!(sample, expected.samples[at].map(|value| value * factor));
            }
            assert_eq!(samples.len(), 13);
            let activated = graph_collect(&mut graph, &mut bridge);
            assert_eq!(activated.len(), 2);
            assert_eq!(activated[0].status, Status::Admitted);
            assert_eq!(activated[1].status, Status::Activated);
            assert_eq!(
                (
                    activated[1].requested,
                    activated[1].effective,
                    activated[1].delivered,
                    activated[1].observed
                ),
                (4, 4, Some(4), 4)
            );
            assert_eq!(activated[1].graph_epoch, rt_runtime::Epoch(2));
            collect(&mut client, &mut bridge);
            assert_eq!(bridge.outstanding(), 0);
            assert_eq!(bridge.stop(), 15);
            assert_eq!(graph.release(scope, slot, generation, false), 0);
            graph_submit(&graph, &mut bridge);
            assert_eq!(bridge.stop(), 15);
            assert_eq!(bridge.render(1), 0);
            let released = graph_collect(&mut graph, &mut bridge);
            assert_eq!(released[0].status, Status::Released);
            assert_eq!(
                released[0].bank.unwrap(),
                crate::graph_wire::Bank {
                    id: crate::graph_transfer::Id {
                        slot: 0,
                        generation: 2
                    },
                    capacity: crate::graph_transfer::Counts {
                        steps: 3,
                        edges: 12,
                        bindings: 2
                    }
                }
            );
            assert_eq!(
                (
                    graph.outstanding(),
                    graph.retained_graphs(),
                    bridge.outstanding(),
                    bridge.retained_graphs()
                ),
                (0, 0, 0, 0)
            );
            let midi: Vec<_> = expected
                .midi
                .iter()
                .flat_map(|(at, packet)| {
                    at.to_le_bytes()
                        .into_iter()
                        .chain((packet.len() as u32).to_le_bytes())
                        .chain(packet.iter().copied())
                })
                .collect();
            assert_eq!(&bridge.journal[..bridge.journal_len], midi);
            assert_eq!(bridge.stop(), 0);
            assert_eq!(bridge.recovered, [256, 16, 16, 0]);
            assert_eq!(&bridge.semantic[..bridge.semantic_len], expected.semantic);
        }
    }
    #[test]
    fn graph_binding_fault_disposal_abandons_credits_without_fabricating_returns() {
        for mode in [1, 2] {
            let mut client = crate::BrowserGraphClient::new(71).unwrap();
            let mut bridge = BrowserBridge::new_graphs(71, mode).unwrap();
            graph_upload(&mut client, &mut bridge);
            assert_eq!(
                bridge.render(128),
                if mode == 1 { 10 } else { HEAP_FAILURE }
            );
            assert_eq!(bridge.fault_witness(), u32::from(mode));
            assert_eq!(bridge.render(1), 10);
            assert_eq!(client.outstanding(), 1);
            assert_eq!(bridge.ingest_graph_payload(), 10);
            assert_eq!(bridge.ingest_graph(), 11);
            assert_eq!(bridge.stop(), 0);
            assert_eq!(bridge.recovered, [256, 16, 16, 0]);
            assert_eq!(client.outstanding(), 1);
        }
    }
    #[test]
    fn diagnostic_journal_exhaustion_does_not_change_music() {
        let mut client = BrowserClient::new(43).unwrap();
        let mut bridge = BrowserBridge::new(43, 0).unwrap();
        submit(&mut client, &mut bridge);
        bridge.journal_len = JOURNAL_BYTES;
        assert_eq!(bridge.render(128), 0);
        assert_eq!(bridge.journal_overflow(), 134);
        assert_eq!(
            &bridge.left[..13],
            &[1., 2., 3., 4., 0.5, 1., 1.5, 2., 0.5, 1., 1.5, 2., 0.]
        );
        collect(&mut client, &mut bridge);
        assert_eq!(bridge.stop(), 0);
    }
}
