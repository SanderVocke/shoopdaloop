//! Native reference for the graph worklet's acknowledged one-sample checkpoint.
use super::{allocation_guard, BrowserBridge, BrowserClient};
use crate::{graph_wire, wire, BrowserGraphClient, Report};

#[derive(Debug)]
pub struct GraphReport {
    pub media: Report,
    pub payloads: Vec<[u8; 96]>,
    pub requests: Vec<[u8; 96]>,
    pub responses: Vec<[u8; 128]>,
    pub primitive_responses: Vec<[u8; 96]>,
    pub frames: u64,
    pub callbacks: u64,
}
impl GraphReport {
    pub fn json(&self) -> String {
        let midi = self
            .media
            .midi
            .iter()
            .map(|(at, packet)| format!("[{at},{packet:?}]"))
            .collect::<Vec<_>>()
            .join(",");
        let transitions = self
            .media
            .transitions
            .iter()
            .map(|(a, b, c)| format!("[{a},{b},{c}]"))
            .collect::<Vec<_>>()
            .join(",");
        format!("{{\"samples\":{:?},\"midi\":[{}],\"transitions\":[{}],\"semanticBytes\":{:?},\"recovered\":[{},{},{},{}],\"payloads\":{:?},\"graphRequests\":{:?},\"graphResponses\":{:?},\"responses\":{:?},\"frames\":{},\"callbacks\":{}}}", self.media.samples, midi, transitions, self.media.semantic, self.media.free_pages, self.media.free_versions, self.media.free_snapshots, self.media.media_references, self.payloads, self.requests, self.responses, self.primitive_responses, self.frames, self.callbacks)
    }
}
fn send(client: &BrowserGraphClient, bridge: &mut BrowserBridge, requests: &mut Vec<[u8; 96]>) {
    bridge.graph.as_mut().unwrap().request = client.request;
    assert_eq!(bridge.ingest_graph(), 0);
    requests.push(client.request);
}
fn collect(
    client: &mut BrowserGraphClient,
    bridge: &mut BrowserBridge,
    responses: &mut Vec<[u8; 128]>,
) {
    for _ in 0..4 {
        let result = bridge.poll_graph_response();
        if result == 0 {
            return;
        }
        assert_eq!(result, 1);
        client.response = bridge.graph.as_ref().unwrap().response;
        assert_eq!(client.receive(), 0);
        responses.push(client.response);
    }
    assert_eq!(bridge.poll_graph_response(), 0);
}
pub fn run_graph(partitions: &[usize]) -> GraphReport {
    assert!(partitions.iter().all(|&frames| frames <= 128));
    assert_eq!(partitions.iter().sum::<usize>(), 128);
    let expected = crate::run(&[13]);
    let mut primitive = BrowserClient::new(u64::MAX).unwrap();
    let mut client = BrowserGraphClient::new(u64::MAX).unwrap();
    let mut bridge = BrowserBridge::new_graphs(u64::MAX, 0).unwrap();
    for (kind, at) in [(1, 0), (2, 4), (3, 12)] {
        assert_eq!(primitive.prepare(kind, at, 0, 0), 0);
        bridge.request = primitive.request;
        assert_eq!(bridge.ingest(), 0);
    }
    let mut payloads = Vec::new();
    for index in 0..client.payload_count() {
        assert_eq!(
            client.prepare_payload(index, 0, 1, 1, u64::from(index) + 1),
            0
        );
        bridge.graph.as_mut().unwrap().payload = client.payload;
        assert_eq!(bridge.ingest_graph_payload(), 0);
        payloads.push(client.payload);
    }
    let mut requests = Vec::new();
    let mut responses = Vec::new();
    assert_eq!(client.stage(0, 1, 0, 1, false), 0);
    send(&client, &mut bridge, &mut requests);
    assert_eq!(bridge.render(0), 0);
    assert_eq!((bridge.frame(), bridge.callbacks()), (0, 0));
    assert_eq!(bridge.render(1), 0);
    let mut samples = vec![[bridge.left[0], bridge.right[0]]];
    collect(&mut client, &mut bridge, &mut responses);
    assert_eq!(responses.len(), 1);
    let staged = graph_wire::Response::decode(&responses[0]).unwrap();
    assert_eq!(staged.status, graph_wire::Status::Staged);
    let [scope, slot, generation] = staged.graph.unwrap().parts();
    assert_eq!(client.activate(scope, slot, generation, 4, 1, 0, false), 0);
    send(&client, &mut bridge, &mut requests);
    for &frames in partitions {
        assert_eq!(bridge.render(frames as u32), 0);
        assert_eq!(bridge.heap, allocation_guard::Counts::default());
        samples.extend((0..frames).map(|at| [bridge.left[at], bridge.right[at]]));
    }
    assert_eq!(samples.len(), 129);
    for (at, &sample) in samples.iter().enumerate() {
        let oracle = expected.samples.get(at).copied().unwrap_or([0.; 2]);
        assert_eq!(
            sample,
            oracle.map(|value| value * if (4..12).contains(&at) { 2. } else { 1. })
        );
    }
    collect(&mut client, &mut bridge, &mut responses);
    assert_eq!(responses.len(), 3);
    let activated = graph_wire::Response::decode(&responses[2]).unwrap();
    assert_eq!(activated.status, graph_wire::Status::Activated);
    assert_eq!(
        (
            activated.requested,
            activated.effective,
            activated.delivered,
            activated.observed,
            activated.graph_epoch.0
        ),
        (4, 4, Some(4), 4, 2)
    );
    let mut primitive_responses = Vec::new();
    let mut transitions = Vec::new();
    for _ in 0..10 {
        let result = bridge.poll_response();
        if result == 0 {
            break;
        }
        assert_eq!(result, 1);
        primitive.response = bridge.response;
        assert_eq!(primitive.receive(), 0);
        primitive_responses.push(primitive.response);
        let receipt = wire::Response::decode(&primitive.response).unwrap();
        assert_eq!(receipt.error, 0);
        if receipt.phase == wire::Phase::Execution {
            assert_eq!(receipt.status, wire::Status::Applied);
            assert_eq!(receipt.requested, receipt.effective);
            transitions.push((
                receipt.requested,
                receipt.delivered.unwrap(),
                receipt.observed,
            ));
        }
    }
    assert_eq!(primitive_responses.len(), 6);
    assert_eq!(transitions, expected.transitions);
    assert_eq!(bridge.stop(), 15);
    assert_eq!(client.release(scope, slot, generation, false), 0);
    send(&client, &mut bridge, &mut requests);
    assert_eq!(bridge.render(1), 0);
    assert_eq!([bridge.left[0], bridge.right[0]], [0.; 2]);
    collect(&mut client, &mut bridge, &mut responses);
    assert_eq!(responses.len(), 4);
    let returned = graph_wire::Response::decode(&responses[3]).unwrap();
    assert_eq!(
        (returned.status, returned.observed),
        (graph_wire::Status::Released, 129)
    );
    let bank = returned.bank.unwrap();
    assert_eq!(
        (
            bank.id.slot,
            bank.id.generation,
            bank.capacity.steps,
            bank.capacity.edges,
            bank.capacity.bindings
        ),
        (0, 2, 3, 12, 2)
    );
    assert_eq!(
        (
            primitive.outstanding(),
            client.outstanding(),
            client.retained_graphs(),
            bridge.outstanding(),
            bridge.retained_graphs()
        ),
        (0, 0, 0, 0, 0)
    );
    let mut midi = Vec::new();
    let mut cursor = 0;
    while cursor < bridge.journal_len {
        let frame = u64::from_le_bytes(bridge.journal[cursor..cursor + 8].try_into().unwrap());
        let length = u32::from_le_bytes(bridge.journal[cursor + 8..cursor + 12].try_into().unwrap())
            as usize;
        midi.push((
            frame,
            bridge.journal[cursor + 12..cursor + 12 + length].to_vec(),
        ));
        cursor += 12 + length;
    }
    assert_eq!(midi, expected.midi);
    let frames = bridge.frame();
    let callbacks = bridge.callbacks();
    assert_eq!(bridge.stop(), 0);
    assert_eq!(bridge.recovered, [256, 16, 16, 0]);
    let semantic = bridge.semantic[..bridge.semantic_len].to_vec();
    assert_eq!(semantic, expected.semantic);
    GraphReport {
        media: Report {
            samples,
            midi,
            transitions,
            semantic,
            free_pages: bridge.recovered[0] as usize,
            free_versions: bridge.recovered[1] as usize,
            free_snapshots: bridge.recovered[2] as usize,
            media_references: bridge.recovered[3] as usize,
        },
        payloads,
        requests,
        responses,
        primitive_responses,
        frames,
        callbacks,
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn acknowledged_graph_worklet_script_has_complete_partition_invariant_native_reference() {
        let reference = super::run_graph(&[128]);
        for partitions in [vec![1; 128], vec![0, 3, 0, 5, 120, 0]] {
            let other = super::run_graph(&partitions);
            assert_eq!(reference.media, other.media);
            assert_eq!(reference.payloads, other.payloads);
            assert_eq!(reference.primitive_responses, other.primitive_responses);
            for (a, b) in reference.requests.iter().zip(&other.requests) {
                let mut a = *a;
                let mut b = *b;
                a[64..68].fill(0);
                b[64..68].fill(0);
                assert_eq!(a, b);
            }
            for (a, b) in reference.responses.iter().zip(&other.responses) {
                let mut a = *a;
                let mut b = *b;
                a[76..80].fill(0);
                b[76..80].fill(0);
                assert_eq!(a, b);
            }
        }
    }
}
