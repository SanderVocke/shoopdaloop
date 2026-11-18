use shoop_graph::*;
use shoop_wasm_test_support::{
    heap::{assert_no_heap, TrackingAllocator},
    shoop_test,
};
#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
use std::borrow::Cow;
#[shoop_test]
fn owned_transfer_preserves_nontrivial_shapes_and_midi_without_execution_state() {
    let id = Id {
        slot: 7,
        generation: 2,
    };
    let mut graph = Graph::default();
    graph.strips.push(StripView {
        id,
        name: Cow::Borrowed("processor"),
        layout: ChannelLayout::DryWet {
            dry_audio: 64,
            wet_audio: 32,
            dry_midi: true,
        },
        controls: StripControls {
            input_gain_db: -3.,
            automatic_monitoring: true,
            ..Default::default()
        },
        default_dry_playback: true,
        processor: Some(ProcessorView {
            type_id: Cow::Borrowed("future-plugin"),
            parameters: Cow::Borrowed(&[0.25, 0.75]),
        }),
    });
    graph.loops.push(LoopView {
        id,
        strip: Some(id),
        layout: ChannelLayout::Direct {
            audio: 0,
            midi: true,
        },
        gain: 0.5,
        balance: 0.,
        boundary_smoothing_frames: 64,
        sync_source: Some(id),
        repeat_sync: true,
        composite: Some(CompositeView {
            kind: CompositeKind::Explicit,
            version: 99,
            unit_frames: 128,
            iterations: 3,
            sample_rate: 48000,
            quantum: 32,
            spans: Cow::Owned(vec![CompositeSpan {
                target: id,
                start: 0,
                end: 2,
                occurrence_start: 0,
                ordinal: 1,
                mode: LoopMode::RecordingDryIntoWet,
            }]),
        }),
    });
    graph.buses.push(BusView {
        id,
        name: Cow::Borrowed("multi"),
        channels: 64,
        master: false,
        gain_db: 3.,
        balance: 0.,
        muted: true,
        processor: Some(ProcessorView {
            type_id: Cow::Borrowed("bus-fx"),
            parameters: Cow::Borrowed(&[1.]),
        }),
    });
    let source = PortId {
        owner: Owner::Host,
        signal: SignalType::Midi,
        role: PortRole::Input,
        index: 55,
    };
    let destination = PortId {
        owner: Owner::Strip(id),
        signal: SignalType::Midi,
        role: PortRole::Control,
        index: 33,
    };
    graph.ports.extend([
        PortView {
            channel: 0,
            id: source,
            name: Cow::Borrowed("controller"),
        },
        PortView {
            channel: 0,
            id: destination,
            name: Cow::Borrowed("CC"),
        },
    ]);
    graph.connections.push(Connection {
        source,
        destination,
    });
    let captured = Graph::capture(&graph);
    assert_eq!(captured, graph);
    assert_no_heap(|| {
        assert_eq!(
            captured
                .strips()
                .next()
                .unwrap()
                .processor
                .unwrap()
                .parameters
                .as_ref(),
            &[0.25, 0.75]
        );
        assert_eq!(
            captured
                .loops()
                .next()
                .unwrap()
                .composite
                .unwrap()
                .spans
                .len(),
            1
        );
        assert_eq!(captured.buses().next().unwrap().channels, 64);
        assert_eq!(captured.ports().count(), 2);
        assert_eq!(captured.connections().count(), 1);
    });
    let encoded = captured.encode().unwrap();
    assert_eq!(Graph::decode(&encoded).unwrap(), graph);
    let mut trailing = encoded.clone();
    trailing.push(0);
    assert!(Graph::decode(&trailing).is_err());
    assert!(Graph::decode(&encoded[..encoded.len() - 1]).is_err());
}
