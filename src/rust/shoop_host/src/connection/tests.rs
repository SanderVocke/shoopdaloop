use super::*;
use shoop_app_protocol::{
    persistence::*,
    persistence_image::{Channel, Image, Loop, Manifest, Payload},
    *,
};
use shoop_wasm_test_support::shoop_test;
use std::{net::TcpListener, sync::Arc};

fn image() -> Arc<Image> {
    let frames = 128 * 1024;
    Arc::new(Image {
        manifest: Manifest::Loop {
            sample_rate: 48000,
            content: Loop {
                id: shoop_app_api::LoopId::from_raw(1),
                name: "Backpressure".into(),
                sync: false,
                frames,
                gain: 1.,
                balance: 0.,
                audio: vec![Channel {
                    object: Some(1),
                    frames,
                    gain: 1.,
                    start_offset: 0,
                    capture_alignment: 0,
                    preplay: 0,
                    recording_started_at: None,
                }],
                midi: Vec::new(),
            },
        },
        payloads: vec![Payload::Audio(Arc::new(vec![0.25; frames as usize]))],
    })
}

fn take(peer: &mut shoop_transport_socket::Port) -> WireMessage {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut bytes = [0; MAX_RECORD_BYTES];
    loop {
        if let Some(length) = peer.take(&mut bytes) {
            return codec::decode(&bytes[..length]).unwrap();
        }
        assert!(Instant::now() < deadline, "peer received no record");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[shoop_test(no_wasm = "the native session host uses local sockets")]
fn a_peer_withholding_acknowledgments_bounds_uploads_without_blocking_controls_or_reuse() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let stream = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (peer, _) = listener.accept().unwrap();
    let mut peer = shoop_transport_socket::Port::from_stream(peer, Default::default()).unwrap();
    let mut connection = Connection {
        port: Port::Socket(
            shoop_transport_socket::Port::from_stream(stream, Default::default()).unwrap(),
        ),
        upload: shoop_app_connection::Upload::new(),
        heartbeat: None,
        last: Instant::now(),
    };
    let mut client = Client::new();
    let authority = Authority {
        lease: LeaseId(1),
        client: ClientId(1),
        generation: ConnectionGeneration(1),
    };
    for transfer in [1, 2] {
        let key = Key {
            transfer: TransferId(transfer),
            epoch: 1,
        };
        connection
            .send(ClientMessage::StageImage {
                authority,
                key,
                target: Target::Loop {
                    loop_id: shoop_app_api::LoopId::from_raw(1),
                    content_revision: 1,
                },
                image: image(),
            })
            .unwrap();
        let mut records = 0;
        let mut bytes = [0; MAX_RECORD_BYTES];
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut pumps = 0;
        while records < MAX_PERSISTENCE_RECORDS_PER_TURN || pumps < 64 {
            assert!(Instant::now() < deadline, "upload did not fill its window");
            connection.poll(&mut client).unwrap();
            while let Some(length) = peer.take(&mut bytes) {
                assert!(
                    matches!(codec::decode(&bytes[..length]).unwrap(), WireMessage::Client(ClientMessage::Persistence { request, .. }) if request.key() == key)
                );
                records += 1;
            }
            assert!(records <= MAX_PERSISTENCE_RECORDS_PER_TURN);
            pumps += 1;
            std::thread::yield_now();
        }
        assert_eq!(records, MAX_PERSISTENCE_RECORDS_PER_TURN);
        assert_eq!(connection.upload.outstanding(), records);
        connection
            .send(ClientMessage::CancelMedia { authority })
            .unwrap();
        assert!(matches!(
            take(&mut peer),
            WireMessage::Client(ClientMessage::CancelMedia { .. })
        ));
        connection
            .send(ClientMessage::Persistence {
                authority,
                request: Request::Cancel { key },
            })
            .unwrap();
        assert!(matches!(
            take(&mut peer),
            WireMessage::Client(ClientMessage::Persistence {
                request: Request::Cancel { .. },
                ..
            })
        ));
        assert!(!connection.upload.active());
        assert_eq!(connection.upload.outstanding(), 0);
    }
}
