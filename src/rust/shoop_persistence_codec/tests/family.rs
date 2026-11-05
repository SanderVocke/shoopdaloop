use shoop_app_api::*;
use shoop_app_protocol::{
    persistence::{self as transfer, Receiver, Target},
    persistence_image as image,
    persistence_transfer::{assemble, UploadStream},
    Authority, ClientId, ConnectionGeneration, LeaseId, Revision, TransferId,
};
use shoop_persistence_codec::{
    codec::{Decoded, Value, Work},
    worker::{self, Command, Response, Worker},
};
use shoop_session::*;
use shoop_wasm_test_support::shoop_test;
use std::sync::Arc;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn capture() -> Arc<image::Image> {
    let mut payloads = Vec::new();
    let members = [96, 384, 1152]
        .into_iter()
        .enumerate()
        .map(|(index, frames)| {
            let id = LoopId::from_raw(index as u64 + 1);
            payloads.push(image::Payload::Audio(Arc::new(vec![0.25; frames as usize])));
            let channel = image::Channel {
                mode: image::ChannelMode::Direct,
                object: Some(payloads.len() as u32),
                frames,
                gain: 1.,
                start_offset: 0,
                capture_alignment: 0,
                preplay: 0,
                recording_started_at: Some("2026-01-01T00:00:00Z".into()),
            };
            payloads.push(image::Payload::Midi(Arc::new(vec![
                MidiEventState {
                    frame: 1,
                    data: vec![0x90, 60, 100].into(),
                },
                MidiEventState {
                    frame: frames as u32 - 1,
                    data: vec![0x80, 60, 0].into(),
                },
            ])));
            image::FamilyMember {
                loop_id: id,
                sync_source: (index > 0).then(|| LoopId::from_raw(index as u64)),
                expected_revision: 7,
                expected_frames: frames,
                fence_content: true,
                restart: index == 0,
                content: Some(image::Loop {
                    recorded_processor_state: None,
                    id,
                    name: "Chain".into(),
                    sync: false,
                    frames,
                    gain: 1.,
                    balance: 0.,
                    audio: vec![channel.clone()],
                    midi: vec![image::MidiChannel {
                        channel: image::Channel {
                            mode: image::ChannelMode::Direct,
                            object: Some(payloads.len() as u32),
                            ..channel
                        },
                        initial: vec![vec![0xb0, 7, 99]],
                    }],
                }),
            }
        })
        .collect();
    Arc::new(image::Image {
        manifest: image::Manifest::FamilyUpdate {
            format: ConversionTarget {
                sample_rate: 1000,
                quantum_frames: 32,
            },
            root: LoopId::from_raw(1),
            members,
        },
        payloads,
    })
}
fn incoming(rate: u32) -> LoopAudio {
    LoopAudio {
        sample_rate: rate,
        channels: vec![LoopAudioChannel {
            label: "New".into(),
            role: "direct".into(),
            samples: vec![0.5; (112 * rate / 1000) as usize],
        }],
    }
}
fn check(image: &image::Image) {
    let image::Manifest::FamilyUpdate { members, .. } = &image.manifest else {
        panic!("expected family");
    };
    assert_eq!(
        members
            .iter()
            .map(|member| member.content.as_ref().unwrap().frames)
            .collect::<Vec<_>>(),
        [128, 448, 1344]
    );
    for (index, member) in members.iter().enumerate() {
        let content = member.content.as_ref().unwrap();
        let image::Payload::Audio(samples) =
            &image.payloads[content.audio[0].object.unwrap() as usize - 1]
        else {
            panic!();
        };
        assert_eq!(samples.len() as u64, content.frames);
        assert!(samples
            .iter()
            .all(|sample| *sample == if index == 0 { 0.5 } else { 0.25 }));
        let midi = &content.midi[0];
        assert_eq!(midi.initial, vec![vec![0xb0, 7, 99]]);
        assert_eq!(midi.channel.frames, [110, 448, 1344][index]);
        let image::Payload::Midi(events) =
            &image.payloads[midi.channel.object.unwrap() as usize - 1]
        else {
            panic!();
        };
        assert_eq!(events[0].frame, 1);
        assert_eq!(events[1].frame, [108, 446, 1342][index]);
        assert_eq!(events[1].data.as_ref(), [0x80, 60, 0]);
    }
}

#[shoop_test]
fn family_chain_combines_rate_and_grid_once_and_cancel_leaves_input_untouched() {
    let base = capture();
    let original = (*base).clone();
    for rate in [1000, 2000] {
        let Value::Image(output) = (Work::ConvertFamily {
            decoded: Decoded::Audio(incoming(rate)),
            base: Arc::clone(&base),
        })
        .run()
        .unwrap() else {
            panic!();
        };
        check(&output);
        assert_eq!(*base, original);
    }
    assert!((Work::ConvertFamily {
        decoded: Decoded::Audio(incoming(1000)),
        base: Arc::clone(&base)
    })
    .run_with_cancel(|| true)
    .is_err());
    assert_eq!(*base, original);
}

fn receive(worker: &mut Worker, command: Command) -> Response {
    worker.receive(worker::decode(&worker::encode(&command).unwrap()).unwrap())
}

#[shoop_test]
fn worker_retains_original_input_across_bounded_family_upload_and_conversion() {
    let authority = Authority {
        lease: LeaseId(1),
        client: ClientId(1),
        generation: ConnectionGeneration(1),
    };
    let key = transfer::Key {
        transfer: TransferId(1),
        epoch: 1,
    };
    let mut worker = Worker::default();
    let bytes = encode_float_wav(&incoming(2000)).unwrap();
    assert!(matches!(
        receive(
            &mut worker,
            Command::BeginFile {
                kind: 3,
                bytes: bytes.len() as u64,
                rate: 1000
            }
        ),
        Response::Ack
    ));
    for (index, chunk) in bytes.chunks(worker::FILE_CHUNK).enumerate() {
        assert!(matches!(
            receive(
                &mut worker,
                Command::FileChunk {
                    offset: (index * worker::FILE_CHUNK) as u64,
                    bytes: chunk.to_vec()
                }
            ),
            Response::Ack
        ));
    }
    let Response::Decoded(summary) = receive(&mut worker, Command::FinishFile) else {
        panic!();
    };
    let Response::Decoded(summary) = receive(
        &mut worker,
        Command::Inspect {
            handle: summary.handle,
            target: ConversionTarget {
                sample_rate: 1000,
                quantum_frames: 32,
            },
            mapping: vec![0],
            update_length: true,
        },
    ) else {
        panic!();
    };
    assert_eq!(summary.frames, Some(224));
    assert_eq!(summary.conversion_required, Some(true));
    let mut upload = UploadStream::new(
        capture(),
        key,
        Target::Session {
            revision: Revision(1),
        },
    )
    .unwrap();
    assert!(matches!(
        receive(
            &mut worker,
            Command::BeginFamily {
                description: upload.description(),
                handle: summary.handle
            }
        ),
        Response::Ack
    ));
    let mut description = None;
    while let Some(part) = upload.next_request().unwrap() {
        match receive(&mut worker, Command::Image(part)) {
            Response::Ack => {}
            Response::Image(output) => description = Some(output),
            response => panic!("family input failed: {response:?}"),
        }
    }
    let mut receiver = Receiver::new(authority, description.unwrap()).unwrap();
    loop {
        let Response::ImageBatch { parts, done } = receive(&mut worker, Command::NextImageBatch)
        else {
            panic!();
        };
        for part in parts {
            if matches!(part, transfer::Request::Begin(_)) {
                continue;
            }
            receiver.apply(authority, part).unwrap();
        }
        if done {
            break;
        }
    }
    let output = assemble(receiver.into_objects().unwrap()).unwrap();
    check(&output);
    assert!(matches!(
        receive(
            &mut worker,
            Command::BeginFamily {
                description: upload.description(),
                handle: summary.handle
            }
        ),
        Response::Failed(_)
    ));
}
