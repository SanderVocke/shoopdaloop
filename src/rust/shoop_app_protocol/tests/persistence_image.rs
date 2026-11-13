use shoop_app_api::{LoopId, MidiEventState};
use shoop_app_protocol::{codec, persistence::*, persistence_image::*, persistence_transfer::*, *};
use shoop_wasm_test_support::shoop_test;
use std::sync::Arc;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
fn key() -> Key {
    Key {
        transfer: TransferId(1),
        epoch: 1,
    }
}
fn authority() -> Authority {
    Authority {
        lease: LeaseId(1),
        client: ClientId(1),
        generation: ConnectionGeneration(1),
    }
}
fn target() -> Target {
    Target::Loop {
        loop_id: LoopId::from_raw(1),
        content_revision: 1,
    }
}
fn channel(object: u32, frames: u64) -> Channel {
    Channel {
        mode: ChannelMode::Direct,
        object: Some(object),
        frames,
        gain: 1.,
        start_offset: 0,
        capture_alignment: 0,
        preplay: 0,
        recording_started_at: None,
    }
}
fn image(samples: usize) -> Arc<Image> {
    Arc::new(Image {
        manifest: Manifest::Loop {
            sample_rate: 48000,
            content: Loop {
                recorded_processor_state: None,
                composite: None,
                sync_source: None,
                repeat_sync: false,
                id: LoopId::from_raw(1),
                name: "Music".to_owned(),
                sync: false,
                frames: samples as u64,
                gain: 0.75,
                balance: 0.,
                audio: vec![channel(1, samples as u64)],
                midi: vec![MidiChannel {
                    channel: channel(2, samples as u64),
                    initial: vec![vec![0xb0, 7, 99]],
                }],
            },
        },
        payloads: vec![
            Payload::Audio(vec![0.25; samples].into()),
            Payload::Midi(Arc::new(vec![
                MidiEventState {
                    frame: 0,
                    data: Arc::from([0x90, 60, 99]),
                },
                MidiEventState {
                    frame: 0,
                    data: Arc::from(
                        vec![0xf0]
                            .into_iter()
                            .chain(vec![1; 2048])
                            .chain([0xf7])
                            .collect::<Vec<_>>(),
                    ),
                },
                MidiEventState {
                    frame: 3,
                    data: Arc::from([0x80, 60, 0]),
                },
            ])),
        ],
    })
}
#[shoop_test]
fn captured_sources_enforce_host_media_metadata_object_and_event_limits() {
    use shoop_app_api::HostResourceLimits;
    for limits in [
        HostResourceLimits {
            media_bytes: 1,
            ..Default::default()
        },
        HostResourceLimits {
            metadata_bytes: 1,
            ..Default::default()
        },
        HostResourceLimits {
            objects: 2,
            ..Default::default()
        },
        HostResourceLimits {
            midi_events: 2,
            ..Default::default()
        },
    ] {
        let mut validation = Validation::for_transfer(image(16)).unwrap();
        while !validation.step(64 * 1024).unwrap() {}
        assert!(matches!(
            Source::from_validated_with_limits(validation.finish().unwrap(), limits),
            Err(Refusal::Overloaded)
        ));
    }
}

#[shoop_test]
fn segmented_images_reconstruct_exact_typed_content_and_random_midi_byte_ranges() {
    let image = image(2048);
    let source = Source::new(Arc::clone(&image)).unwrap();
    assert!(Arc::ptr_eq(source.image(), &image));
    let mut stream = UploadStream::new(Arc::clone(&image), key(), target()).unwrap();
    let Request::Begin(description) = stream.next_request().unwrap().unwrap() else {
        panic!()
    };
    let mut receiver = Receiver::new(authority(), description).unwrap();
    while let Some(request) = stream.next_request().unwrap() {
        let wire = WireMessage::Client(ClientMessage::Persistence {
            authority: authority(),
            request: request.clone(),
        });
        let mut bytes = [0; shoop_transport::MAX_RECORD_BYTES];
        let count = codec::encode(&wire, &mut bytes).unwrap();
        assert_eq!(codec::decode(&bytes[..count]).unwrap(), wire);
        receiver.apply(authority(), request).unwrap();
    }
    let rebuilt = assemble(receiver.into_objects().unwrap()).unwrap();
    assert_eq!(*rebuilt, *image);
    let midi = source.object(2).unwrap();
    let expected = (0..midi.bytes)
        .step_by(CHUNK_BYTES)
        .flat_map(|offset| source.chunk(2, offset).unwrap())
        .collect::<Vec<_>>();
    for offset in [midi.bytes - 1, 11, 0, 1023, 12, 27, 512, 1024] {
        let chunk = source.chunk(2, offset).unwrap();
        assert_eq!(
            &expected[offset as usize..offset as usize + chunk.len()],
            chunk
        );
    }
    assert_eq!(source.chunk(2, midi.bytes), Err(Refusal::Invalid));
}
#[shoop_test]
fn images_above_the_old_media_view_limit_use_only_bounded_records() {
    let image = image(8 * 1024 * 1024 + 1);
    let mut stream = UploadStream::new(Arc::clone(&image), key(), target()).unwrap();
    let Request::Begin(description) = stream.next_request().unwrap().unwrap() else {
        panic!()
    };
    assert!(description.media_bytes > MAX_MEDIA_BYTES as u64);
    let mut receiver = Receiver::new(authority(), description).unwrap();
    while let Some(request) = stream.next_request().unwrap() {
        if let Request::Chunk { bytes, .. } = &request {
            assert!(bytes.len() <= CHUNK_BYTES);
        }
        receiver.apply(authority(), request).unwrap();
    }
    let rebuilt = assemble(receiver.into_objects().unwrap()).unwrap();
    assert_eq!(*rebuilt, *image);
}
#[shoop_test]
fn typed_media_cannot_enter_an_ordinary_wire_record_and_bad_references_refuse() {
    let image = image(16);
    let local = WireMessage::Client(ClientMessage::StageImage {
        authority: authority(),
        key: key(),
        target: target(),
        image: Arc::clone(&image),
    });
    assert_eq!(
        codec::encode(&local, &mut [0; shoop_transport::MAX_RECORD_BYTES]),
        Err(shoop_transport::CodecError::Invalid)
    );
    let mut invalid = (*image).clone();
    let Manifest::Loop { content, .. } = &mut invalid.manifest else {
        panic!()
    };
    content.audio[0].object = Some(2);
    assert!(invalid.validate().is_err());
    let Manifest::Loop { content, .. } = &mut invalid.manifest else {
        panic!()
    };
    content.audio[0].object = Some(1);
    content.audio[0].frames = u64::MAX;
    assert!(invalid.validate().is_err());
}
#[shoop_test]
fn validation_and_assembly_yield_before_full_media_and_keep_immutable_ownership() {
    let image = image(256 * 1024);
    assert!(matches!(
        Validation::new(Arc::clone(&image)).unwrap().finish(),
        Err(Refusal::NotReady)
    ));
    let mut validation = Validation::for_transfer(Arc::clone(&image)).unwrap();
    assert!(!validation.step(64 * 1024).unwrap());
    let mut turns = 1;
    while !validation.step(64 * 1024).unwrap() {
        turns += 1;
    }
    assert!(turns >= 16);
    let validated = validation.finish().unwrap();
    assert!(Arc::ptr_eq(validated.image(), &image));
    let source = Source::from_validated(validated).unwrap();
    assert!(Arc::ptr_eq(source.image(), &image));
    let mut receiver = Receiver::new(authority(), source.description(key(), target())).unwrap();
    let mut stream = UploadStream::new(Arc::clone(&image), key(), target()).unwrap();
    stream.next_request().unwrap();
    while let Some(request) = stream.next_request().unwrap() {
        receiver.apply(authority(), request).unwrap();
    }
    let mut assembly = Assembly::new(receiver.into_objects().unwrap()).unwrap();
    for _ in 0..15 {
        assert!(assembly.step(64 * 1024).unwrap().is_none());
    }
    let rebuilt = loop {
        if let Some(image) = assembly.step(64 * 1024).unwrap() {
            break image.into_image();
        }
    };
    assert_eq!(*rebuilt, *image);
    let mut malformed = (*image).clone();
    let Payload::Audio(samples) = &mut malformed.payloads[0] else {
        panic!()
    };
    *Arc::make_mut(samples).last_mut().unwrap() = f32::NAN;
    let mut validation = Validation::new(Arc::new(malformed)).unwrap();
    assert!(!validation.step(64 * 1024).unwrap());
    loop {
        match validation.step(64 * 1024) {
            Err(Refusal::Invalid) => break,
            Ok(false) => {}
            _ => panic!("invalid trailing media was accepted"),
        }
    }
    assert_eq!(
        image.validate().unwrap(),
        source.description(key(), target()).media_bytes
    );
}
