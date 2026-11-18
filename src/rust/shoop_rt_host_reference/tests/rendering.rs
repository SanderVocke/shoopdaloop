use rt_runtime::{
    buffer::{AudioBlock, MidiBlock},
    host::Renderer,
    host_adapter::QuantumAdapter,
};
use shoop_rt_host_reference::{Endpoint, ReferenceRenderer};
use shoop_rt_protocol::{
    immediate::{Batch, Code, Command, GraphLoop, TransitionMode, TransitionPlan},
    DriverKind, DriverRunState, DriverStatus, EngineMessage,
};
use shoop_rt_transport::{carrier::InProcess, client::ProtocolClient};
use shoop_wasm_test_support::{
    heap::{assert_no_heap, TrackingAllocator},
    shoop_test,
};
#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
type Client = ProtocolClient<InProcess<Endpoint>>;
const Q: usize = 16;
fn step(renderer: &mut QuantumAdapter<ReferenceRenderer>, level: f32) -> [f32; Q] {
    let input = [level; Q];
    let mut left = [0.; Q];
    let mut right = [0.; Q];
    assert_no_heap(|| {
        renderer
            .process(
                AudioBlock {
                    frame: renderer.next_frame(),
                    frames: Q,
                    inputs: &[&input],
                    outputs: &mut [&mut left, &mut right],
                },
                MidiBlock {
                    inputs: &[],
                    outputs: &mut [],
                },
            )
            .unwrap()
    });
    assert_eq!(left, right);
    left
}
fn response(
    client: &mut Client,
    renderer: &mut QuantumAdapter<ReferenceRenderer>,
) -> shoop_rt_protocol::immediate::Response {
    for _ in 0..100 {
        while let Some(message) = client.poll() {
            if let EngineMessage::ImmediateResponse(response) = message {
                return response;
            }
        }
        step(renderer, 0.25);
    }
    panic!("reference command did not finish with independent render demand");
}
fn send(
    client: &mut Client,
    renderer: &mut QuantumAdapter<ReferenceRenderer>,
    commands: &[Command],
    revision: u64,
) {
    client
        .immediate(Batch::new(revision, revision, commands).unwrap())
        .unwrap();
    let result = response(client, renderer);
    assert_eq!(result.header.code(), Code::Applied);
}
fn transition(
    client: &mut Client,
    renderer: &mut QuantumAdapter<ReferenceRenderer>,
    mode: TransitionMode,
) {
    send(
        client,
        renderer,
        &[Command::Transition(
            1,
            TransitionPlan {
                now: Some(mode),
                first: None,
                second: None,
            },
        )],
        1,
    );
}
#[shoop_test]
fn actual_input_records_and_plays_through_quantum_adapter_without_callback_allocations() {
    let prepared = shoop_rt_host_reference::prepare(
        Q as u32,
        DriverStatus {
            kind: DriverKind::Dummy,
            state: DriverRunState::Running,
            sample_rate: 48000,
            buffer_frames: Q as u32,
        },
        1,
    )
    .unwrap();
    let mut renderer = prepared.renderer;
    let mut client = ProtocolClient::new(InProcess::new(
        prepared.endpoint,
        shoop_rt_transport::carrier::Outbox::new(64, 8, 0),
    ));
    client.connect().unwrap();
    while client.poll().is_some() {}
    let mut snapshot = vec![0; client.carrier_mut().endpoint().snapshot_capacity()];
    let (_, len) = client
        .carrier_mut()
        .endpoint()
        .read_snapshot(&mut snapshot)
        .expect("snapshot before render demand");
    assert_eq!(
        shoop_rt_protocol::snapshot::Header::read(&snapshot[..len])
            .unwrap()
            .frame,
        0
    );
    assert_eq!(renderer.completed_frame().0, 0);
    send(
        &mut client,
        &mut renderer,
        &[
            Command::BeginGraph {
                quantum: Q as u32,
                loops: 2,
            },
            Command::ConfigureLoop(GraphLoop {
                slot: 0,
                generation: 1,
                source: None,
                input: None,
                gain: 1.,
                output: false,
                repeat_sync: false,
            }),
            Command::ConfigureLoop(GraphLoop {
                slot: 1,
                generation: 1,
                source: None,
                input: Some(0),
                gain: 1.,
                output: true,
                repeat_sync: false,
            }),
            Command::ActivateGraph,
        ],
        0,
    );
    transition(&mut client, &mut renderer, TransitionMode::Recording);
    for _ in 0..8 {
        step(&mut renderer, 0.25);
        while client.poll().is_some() {}
    }
    transition(&mut client, &mut renderer, TransitionMode::Stopped);
    transition(&mut client, &mut renderer, TransitionMode::Playing);
    for _ in 0..4 {
        step(&mut renderer, 0.);
        while client.poll().is_some() {}
    }
    assert_eq!(step(&mut renderer, 0.), [0.25; Q]);
    send(&mut client, &mut renderer, &[Command::LoopGain(1, 0.5)], 1);
    for _ in 0..4 {
        step(&mut renderer, 0.);
        while client.poll().is_some() {}
    }
    assert_eq!(step(&mut renderer, 0.), [0.125; Q]);
    let mut too_large = vec![Command::BeginGraph {
        quantum: Q as u32,
        loops: 21,
    }];
    too_large.extend((0..21).map(|slot| {
        Command::ConfigureLoop(GraphLoop {
            slot,
            generation: 1,
            source: None,
            input: Some(0),
            output: true,
            gain: 1.,
            repeat_sync: false,
        })
    }));
    too_large.push(Command::ActivateGraph);
    client
        .immediate(Batch::new(1, 1, &too_large).unwrap())
        .unwrap();
    assert_eq!(
        response(&mut client, &mut renderer).header.code(),
        Code::BufferCapacity
    );
    assert_eq!(step(&mut renderer, 0.), [0.125; Q]);
}
