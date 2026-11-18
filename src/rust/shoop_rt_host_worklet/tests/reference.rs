#![cfg(feature = "fpga-reference")]
#[path = "../../rt_fpga_compiler/tests/support/mono.rs"]
mod mono;
use shoop_rt_host_worklet::{
    framing,
    reference::{Endpoint, ImagePort},
    Inbound, Outbound, Quantum, WorkletConfig, WorkletHost,
};
use shoop_rt_protocol::{
    immediate::{Batch, Code, Command, GraphLoop, TransitionMode, TransitionPlan},
    DriverKind, DriverRunState, DriverStatus, EngineMessage, MemoryReport, Telemetry,
};
use shoop_rt_transport::{
    carrier::{CarrierError, InProcess, Outbox, RecordPort},
    client::ProtocolClient,
};
use shoop_wasm_test_support::{
    heap::{assert_no_heap, TrackingAllocator},
    shoop_test,
};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
const Q: usize = 16;
#[derive(Default)]
struct Wire {
    to_worklet: VecDeque<Vec<u8>>,
    to_main: VecDeque<Vec<u8>>,
    images: VecDeque<Vec<u8>>,
    ready: VecDeque<(u64, bool)>,
}
#[derive(Clone, Default)]
struct Port(Rc<RefCell<Wire>>);
impl RecordPort for Port {
    fn post(&mut self, record: &[u8]) -> Result<(), CarrierError> {
        self.0.borrow_mut().to_worklet.push_back(record.to_vec());
        Ok(())
    }
    fn take(&mut self, buf: &mut [u8]) -> Option<usize> {
        let bytes = self.0.borrow_mut().to_main.pop_front()?;
        buf[..bytes.len()].copy_from_slice(&bytes);
        Some(bytes.len())
    }
}
impl ImagePort for Port {
    fn upload(&mut self, bytes: &[u8]) -> Result<(), CarrierError> {
        self.0.borrow_mut().images.push_back(bytes.to_vec());
        Ok(())
    }
    fn image_ready(&mut self) -> Option<(u64, bool)> {
        self.0.borrow_mut().ready.pop_front()
    }
}
struct Browser {
    host: WorkletHost,
    port: Port,
    client: ProtocolClient<InProcess<Endpoint<Port>>>,
    frame: u64,
    published: Vec<shoop_rt_protocol::snapshot::Header>,
    usage: Option<MemoryReport>,
}
impl Browser {
    fn new() -> Self {
        let mut config = WorkletConfig::new(shoop_rt_engine::EngineDescription {
            quantum: Q,
            max_frames: Q,
            sample_rate: 1000,
            host: shoop_rt_engine::HostPorts::numbered(1, 2, 0),
            ..Default::default()
        });
        config.backend = shoop_rt_host_worklet::config::Backend::RustReference;
        config.reference_memory_mb = 1;
        let host = WorkletHost::prepare(&config).unwrap();
        let port = Port::default();
        let endpoint = Endpoint::new(
            port.clone(),
            Q as u32,
            DriverStatus {
                kind: DriverKind::WebAudio,
                state: DriverRunState::Running,
                sample_rate: 1000,
                buffer_frames: Q as u32,
            },
        )
        .unwrap();
        let mut client = ProtocolClient::new(InProcess::new(endpoint, Outbox::new(64, 8, 0)));
        client.connect().unwrap();
        while client.poll().is_some() {}
        client.observe_snapshots(true).unwrap();
        Self {
            host,
            port,
            client,
            frame: 0,
            published: Vec::new(),
            usage: None,
        }
    }
    fn step(&mut self) -> Vec<shoop_rt_protocol::immediate::Response> {
        while let Some(bytes) = self.port.0.borrow_mut().images.pop_front() {
            self.host.load_reference_image(&bytes).unwrap();
        }
        while let Some(bytes) = self.port.0.borrow_mut().to_worklet.pop_front() {
            assert_eq!(self.host.write(Inbound::Records, &bytes), bytes.len());
        }
        self.host.audio_input_mut().fill(0.25);
        assert_no_heap(|| {
            self.host.process(Quantum {
                frames: Q,
                input_channels: 1,
                frame: self.frame,
                load: None,
            })
        })
        .unwrap();
        self.frame += Q as u64;
        if let Some(ready) = self.host.reference_image_ready() {
            self.port.0.borrow_mut().ready.push_back(ready);
        }
        let bytes = self.host.read(Outbound::Records, <[u8]>::to_vec);
        for record in framing::records(&bytes) {
            self.port
                .0
                .borrow_mut()
                .to_main
                .push_back(record.unwrap().to_vec());
        }
        let mut responses = Vec::new();
        while let Some(message) = self.client.poll() {
            match message {
                EngineMessage::ImmediateResponse(r) => responses.push(r),
                EngineMessage::Telemetry(Telemetry::Memory(report)) => {
                    assert!(report.valid());
                    self.usage = Some(report);
                }
                EngineMessage::Snapshot(chunk) if chunk.offset == 0 => {
                    self.published.push(
                        shoop_rt_protocol::snapshot::Header::read(
                            &chunk.bytes[..usize::from(chunk.len)],
                        )
                        .unwrap(),
                    );
                }
                _ => {}
            }
        }
        responses
    }
    fn send(&mut self, batch: Batch) {
        self.client.immediate(batch).unwrap();
        for _ in 0..100 {
            let r = self.step();
            if !r.is_empty() {
                assert_eq!(r[0].header.code(), Code::Applied);
                for record in r[0].records() {
                    assert_eq!(record.code(), Code::Applied);
                }
                return;
            }
        }
        panic!("browser command did not complete");
    }
}
fn graph(rev: u64) -> Batch {
    Batch::new(
        rev,
        rev,
        &[
            Command::BeginGraph {
                quantum: Q as u32,
                loops: 1,
            },
            Command::ConfigureLoop(GraphLoop {
                slot: 0,
                generation: 1,
                source: None,
                input: Some(0),
                gain: 1.,
                output: true,
                repeat_sync: false,
            }),
            Command::ActivateGraph,
        ],
    )
    .unwrap()
}
#[shoop_test]
fn separate_heap_image_loading_records_plays_and_retains_media_without_render_allocation() {
    let mut browser = Browser::new();
    let usage = |browser: &Browser, name: &str| {
        browser
            .usage
            .unwrap()
            .resources
            .into_iter()
            .flatten()
            .find(|resource| resource.name.as_str() == name)
            .unwrap()
    };
    browser.step();
    assert_eq!(usage(&browser, "Audio storage").used, 0);
    assert_eq!(usage(&browser, "Audio storage").capacity, Some(2048));
    browser.send(graph(0));
    let transition = |mode| {
        Batch::new(
            1,
            1,
            &[Command::Transition(
                0,
                TransitionPlan {
                    now: Some(mode),
                    first: None,
                    second: None,
                },
            )],
        )
        .unwrap()
    };
    browser.send(transition(TransitionMode::Recording));
    browser.step();
    browser.send(transition(TransitionMode::Playing));
    for _ in 0..16 {
        browser.step();
    }
    assert_eq!(usage(&browser, "State slots").used, 2);
    assert!(usage(&browser, "Audio storage").used > 0);
    assert!(browser.host.audio_output().iter().all(|s| *s == 0.25));
    assert!(browser
        .published
        .iter()
        .any(|h| h.graph == 1 && h.loops == 1 && h.frame > 0));
    browser.send(graph(1));
    for _ in 0..4 {
        browser.step();
    }
    assert!(browser
        .published
        .iter()
        .any(|h| h.graph == 2 && h.loops == 1));
    assert!(browser.host.audio_output().iter().all(|s| *s == 0.25));
}
#[shoop_test]
fn bounded_loader_rejects_invalid_payload_and_marlin_selection() {
    assert!(shoop_rt_host_worklet::reference::decode_image(b"{}").is_err());
    assert!(shoop_rt_host_worklet::reference::decode_image(&vec![
        0;
        shoop_rt_host_worklet::reference::MAX_IMAGE_BYTES
            + 1
    ])
    .is_err());
    let mut config = WorkletConfig::new(Default::default());
    config.backend = shoop_rt_host_worklet::config::Backend::RtlSim;
    assert!(WorkletHost::prepare(&config).is_err());
}

#[shoop_test]
fn browser_uploads_large_graphs_in_bounded_groups_and_reports_resource_refusals() {
    let mut b = Browser::new();
    let mut commands = vec![Command::BeginGraph {
        quantum: Q as u32,
        loops: 31,
    }];
    commands.extend((100..131).map(|slot| {
        Command::ConfigureLoop(GraphLoop {
            slot,
            generation: 1,
            source: None,
            input: Some(0),
            output: false,
            gain: 1.,
            repeat_sync: false,
        })
    }));
    commands.push(Command::ActivateGraph);
    for chunk in commands.chunks(shoop_rt_protocol::immediate::COMMANDS) {
        b.send(Batch::new(0, 0, chunk).unwrap());
    }
    commands.clear();
    commands.push(Command::BeginGraph {
        quantum: Q as u32,
        loops: 22,
    });
    commands.extend((200..222).map(|slot| {
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
    commands.push(Command::ActivateGraph);
    b.client
        .immediate(Batch::new(1, 1, &commands).unwrap())
        .unwrap();
    for _ in 0..100 {
        if let Some(response) = b.step().first() {
            assert_eq!(response.header.code(), Code::BufferCapacity);
            // The refused replacement leaves the active graph addressable.
            b.send(Batch::new(1, 1, &[Command::LoopGain(100, 0.5)]).unwrap());
            return;
        }
    }
    panic!("browser graph refusal did not finish");
}

#[shoop_test]
fn browser_compiles_complete_graph_views_and_refuses_midi_without_losing_playback() {
    use rt_fpga_compiler::Identity;
    let image = mono::session(
        &(vec![mono::MonoLoop {
            id: Identity {
                slot: 0,
                generation: 1,
            },
            source: None,
            input: true,
            output: true,
            gain: 1.,
            repeat_sync: false,
        }]),
    );
    let mut b = Browser::new();
    for batch in shoop_rt_protocol::graph::upload(&image, Q as u32, 0).unwrap() {
        b.send(batch);
    }
    let transition = |mode| {
        Batch::new(
            1,
            1,
            &[Command::Transition(
                0,
                TransitionPlan {
                    now: Some(mode),
                    first: None,
                    second: None,
                },
            )],
        )
        .unwrap()
    };
    b.send(transition(TransitionMode::Recording));
    b.step();
    b.send(transition(TransitionMode::Playing));
    for _ in 0..4 {
        b.step();
    }
    assert!(b.host.audio_output().iter().all(|s| *s == 0.25));
    let mut unsupported = image.clone();
    unsupported.strips[0].layout = rt_fpga_compiler::shoop_graph::ChannelLayout::Direct {
        audio: 1,
        midi: true,
    };
    for batch in shoop_rt_protocol::graph::upload(&unsupported, Q as u32, 1).unwrap() {
        let terminal = batch.commands().contains(&Command::ActivateGraph);
        b.client.immediate(batch).unwrap();
        let mut done = false;
        for _ in 0..100 {
            if let Some(response) = b.step().first() {
                assert_eq!(
                    response.header.code(),
                    if terminal {
                        Code::GraphMidiUnsupported
                    } else {
                        Code::Applied
                    }
                );
                done = true;
                break;
            }
        }
        assert!(done);
    }
    for _ in 0..4 {
        b.step();
    }
    assert!(b.host.audio_output().iter().all(|s| *s == 0.25));
    for batch in shoop_rt_protocol::graph::upload(&image, Q as u32, 1).unwrap() {
        b.send(batch);
    }
}
