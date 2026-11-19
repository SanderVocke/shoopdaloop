#![cfg(all(not(target_arch = "wasm32"), feature = "carla-worker"))]

use shoop_carla::{
    carla_shared_memory::SharedBlockTransport, processor::CarlaProcessor, state::encode_state,
    subprocess::SupervisedCarlaProcessor,
};
use shoop_plugin_protocol::{
    read_frame, write_frame, BlockSequence, CarlaChainType, CarlaProcessorLifecycle, ChainId,
    ControlRequest, ControlRequestKind, ControlResponse, ControlResponseKind, MidiEvent,
    ParentToWorker, ProcessGeneration, ProtocolErrorCode, PrototypeBlock, RequestId,
    WorkerToParent,
};
use shoop_wasm_test_support::shoop_test;
use std::{
    net::{Ipv4Addr, TcpListener, TcpStream, UdpSocket},
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(5);
const CHAIN: ChainId = ChainId(41);
const GENERATION: ProcessGeneration = ProcessGeneration(73);

fn runtime_available() -> bool {
    match shoop_carla::native::carla_runtime_availability() {
        Ok(()) => true,
        Err(error) => {
            assert!(
                std::env::var_os("SHOOP_REQUIRE_CARLA_TESTS").is_none(),
                "{error}"
            );
            eprintln!("skipping Carla child test: {error}");
            false
        }
    }
}

struct Worker(Child);

impl Drop for Worker {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn send(
    stream: &mut TcpStream,
    id: u64,
    generation: ProcessGeneration,
    kind: ControlRequestKind,
) -> ControlResponseKind {
    write_frame(
        stream,
        &ParentToWorker::Control(ControlRequest {
            request_id: RequestId(id),
            chain_id: CHAIN,
            generation,
            kind,
        }),
    )
    .unwrap();
    let WorkerToParent::Control(response) = read_frame(stream).unwrap() else {
        panic!("worker sent an unexpected audio result");
    };
    response.validate().unwrap();
    assert_eq!(response.request_id, RequestId(id));
    assert_eq!(response.chain_id, CHAIN);
    assert_eq!(response.generation, GENERATION);
    response.kind
}

#[shoop_test]
fn companion_carla_child_processes_socket_and_shared_audio_and_midi() {
    if !runtime_available() {
        return;
    }
    const FRAMES: usize = 64;
    let nonce = [0x53; 32];
    let mut transport = SharedBlockTransport::create(GENERATION, &nonce).unwrap();
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut worker = Worker(
        Command::new(env!("CARGO_BIN_EXE_shoop_rt_host_process"))
            .arg("--carla-worker")
            .arg("--carla-worker-address")
            .arg(listener.local_addr().unwrap().to_string())
            .arg("--carla-worker-nonce=5353535353535353535353535353535353535353535353535353535353535353")
            .arg("--carla-worker-chain-id")
            .arg(CHAIN.0.to_string())
            .arg("--carla-worker-generation")
            .arg(GENERATION.0.to_string())
            .arg("--carla-worker-shared-memory")
            .arg(transport.path())
            .spawn()
            .unwrap(),
    );
    let start = Instant::now();
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, peer)) => {
                assert!(peer.ip().is_loopback());
                break stream;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    worker.0.try_wait().unwrap().is_none(),
                    "Carla child exited before startup"
                );
                assert!(
                    start.elapsed() < TIMEOUT,
                    "Carla child connection timed out"
                );
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("Carla connection failed: {error}"),
        }
    };
    stream.set_read_timeout(Some(TIMEOUT)).unwrap();
    stream.set_write_timeout(Some(TIMEOUT)).unwrap();
    let WorkerToParent::Control(ControlResponse {
        request_id: RequestId(1),
        chain_id: CHAIN,
        generation: GENERATION,
        kind: ControlResponseKind::Handshake(hello),
    }) = read_frame(&mut stream).unwrap()
    else {
        panic!("invalid Carla handshake");
    };
    hello.validate(&nonce, GENERATION).unwrap();
    let stale = send(
        &mut stream,
        2,
        ProcessGeneration(GENERATION.0 - 1),
        ControlRequestKind::Ping,
    );
    assert!(
        matches!(stale, ControlResponseKind::Error(error) if error.code == ProtocolErrorCode::StaleGeneration)
    );
    assert_eq!(
        send(
            &mut stream,
            3,
            GENERATION,
            ControlRequestKind::Instantiate {
                chain_type: CarlaChainType::Rack,
                sample_rate: 48_000,
                nominal_buffer_size: FRAMES as u32,
            }
        ),
        ControlResponseKind::Ack
    );
    let ControlResponseKind::State(state) =
        send(&mut stream, 4, GENERATION, ControlRequestKind::SaveState)
    else {
        panic!("missing Carla Native state");
    };
    assert!(state.starts_with("shoop-carla-native-state:2:rack:"));
    assert_eq!(
        send(
            &mut stream,
            5,
            GENERATION,
            ControlRequestKind::RestoreState(state)
        ),
        ControlResponseKind::Ack
    );
    assert_eq!(
        send(
            &mut stream,
            6,
            GENERATION,
            ControlRequestKind::SetActive(true)
        ),
        ControlResponseKind::Ack
    );

    let audio_inputs = [vec![0.125; FRAMES], vec![-0.75; FRAMES]];
    let midi = MidiEvent {
        frame_offset: 17,
        data: vec![0x90, 60, 100],
    };
    write_frame(
        &mut stream,
        &ParentToWorker::Process(PrototypeBlock {
            sequence: BlockSequence(1),
            generation: GENERATION,
            frames: FRAMES as u32,
            audio_inputs: audio_inputs.to_vec(),
            midi_inputs: vec![midi.clone()],
        }),
    )
    .unwrap();
    let WorkerToParent::Process(result) = read_frame(&mut stream).unwrap() else {
        panic!("missing Carla DSP result");
    };
    result.validate().unwrap();
    assert_eq!(
        (result.sequence, result.generation),
        (BlockSequence(1), GENERATION)
    );
    assert_eq!(result.audio_outputs, audio_inputs);
    assert_eq!(result.midi_outputs, [midi.clone()]);

    let token = transport
        .submit(
            BlockSequence(2),
            FRAMES,
            &audio_inputs,
            2,
            &[vec![(midi.frame_offset, midi.data.clone())]],
            &[1],
        )
        .unwrap();
    let notification = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    notification
        .send_to(&nonce[..16], (Ipv4Addr::LOCALHOST, hello.notification_port))
        .unwrap();
    let mut outputs = [vec![0.; FRAMES], vec![0.; FRAMES]];
    let mut events = Vec::new();
    transport
        .wait_and_copy(token, Instant::now() + TIMEOUT, &mut outputs, &mut events)
        .unwrap();
    assert_eq!(outputs, audio_inputs);
    assert_eq!(events, [midi]);

    let ControlResponseKind::Status(status) =
        send(&mut stream, 7, GENERATION, ControlRequestKind::Status)
    else {
        panic!("missing Carla status");
    };
    assert!(status.active && status.ready);
    assert_eq!(status.processed_blocks, 2);
    assert_eq!(
        send(&mut stream, 8, GENERATION, ControlRequestKind::Shutdown),
        ControlResponseKind::Ack
    );
    let start = Instant::now();
    loop {
        if let Some(exit) = worker.0.try_wait().unwrap() {
            assert!(exit.success(), "Carla child exited with {exit}");
            break;
        }
        assert!(start.elapsed() < TIMEOUT, "Carla child shutdown timed out");
        thread::sleep(Duration::from_millis(1));
    }
}

#[shoop_test]
fn a_lost_carla_child_recovers_its_checkpoint_while_another_chain_keeps_running() {
    if !runtime_available() {
        return;
    }
    const FRAMES: usize = 256;
    let launch = |id| {
        let mut chain = SupervisedCarlaProcessor::launch(
            env!("CARGO_BIN_EXE_shoop_rt_host_process"),
            CarlaChainType::Rack,
            48_000,
            FRAMES as u32,
            ChainId(id),
        )
        .unwrap();
        assert!(chain.is_ready(), "{:?}", chain.crash_summary());
        chain.set_active(true);
        assert!(chain.is_active());
        chain
    };
    let mut recover = launch(100);
    let mut unaffected = launch(101);
    let project = include_str!("../../shoop_engine/test_data/carla_legacy_rack_loaded_project.xml")
        .replacen("<Value>1</Value>", "<Value>0.5</Value>", 1);
    let checkpoint = encode_state(CarlaChainType::Rack, project.as_bytes()).unwrap();
    recover.restore_state(&checkpoint).unwrap();
    let confirm_audio = |chain: &mut SupervisedCarlaProcessor, expected: f32| {
        for _ in 0..12 {
            chain.audio_input_mut(0).unwrap()[..FRAMES].fill(0.25);
            chain.audio_input_mut(1).unwrap()[..FRAMES].fill(-0.25);
            chain.process(FRAMES).unwrap();
            let left = &chain.audio_output(0).unwrap()[..FRAMES];
            let right = &chain.audio_output(1).unwrap()[..FRAMES];
            if left
                .iter()
                .all(|sample| (*sample - expected).abs() < 0.0001)
                && right
                    .iter()
                    .all(|sample| (*sample + expected).abs() < 0.0001)
            {
                return;
            }
            thread::sleep(Duration::from_millis(1));
        }
        panic!(
            "worker audio was {}, expected {expected}",
            chain.audio_output(0).unwrap()[FRAMES - 1]
        );
    };
    confirm_audio(&mut recover, 0.125);
    confirm_audio(&mut unaffected, 0.25);
    let generation = recover.generation();
    let unaffected_generation = unaffected.generation();
    recover.terminate_worker_for_test().unwrap();
    assert!(!recover.is_ready());
    assert_eq!(recover.lifecycle(), CarlaProcessorLifecycle::Crashed);
    confirm_audio(&mut unaffected, 0.25);
    recover.recover(false).unwrap();
    assert_eq!(recover.generation(), generation + 1);
    assert_eq!(unaffected.generation(), unaffected_generation);
    assert!(recover.is_active());
    confirm_audio(&mut recover, 0.125);
    let logs = recover.generation_logs();
    assert_eq!(
        logs.iter().map(|log| log.generation).collect::<Vec<_>>(),
        [generation, generation + 1]
    );
    recover.clear_logs();
    let logs = recover.generation_logs();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].generation, generation + 1);
    assert!(logs[0].stdout.is_empty() && logs[0].stderr.is_empty());
}
