use shoop_rt_protocol::{codec::*, host::*, *};
use shoop_wasm_test_support::shoop_test;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[shoop_test]
fn bounded_host_messages_round_trip_and_do_not_decode_as_engine_messages() {
    let engine = EngineSettings {
        history: InputHistorySettings::default(),
        quantum: 32,
        max_frames: 1,
        strip_channels: 2,
        master_channels: 3,
        buses: 8,
        bus_channels: 8,
        smoothing_frames: 4,
        smoothing_limit_frames: 5,
        ramp_frames: 6,
        control_lead_frames: 7,
        resource_pages: 8,
        page_bytes: 9,
        midi_events: 10,
        midi_bytes: 11,
        pending_transitions: 12,
        reliable_credits: 13,
        bulk_credits: 14,
        telemetry_interval_frames: 15,
    };
    let text = Text::new("entrée Δ").unwrap();
    let driver = DriverSettings {
        kind: DriverKind::Cpal,
        host: text,
        input: DeviceSelection::None,
        output: DeviceSelection::Named(text),
        sample_rate: 48000,
        buffer_frames: 256,
        audio_inputs: 2,
        audio_outputs: 2,
        midi_inputs: 1,
        midi_outputs: 1,
        client_name: text,
        dummy_input_ramp: false,
    };
    let requests = [
        HostRequest::Hello {
            version: HOST_VERSION,
        },
        HostRequest::Discover {
            kind: DiscoverKind::AudioOutputs,
            index: 3,
            host: text,
        },
        HostRequest::ConfigureEngine(engine),
        HostRequest::ConfigureDriver(driver),
        HostRequest::AddSegment {
            strips: 8,
            loops: 32,
        },
        HostRequest::AddMidiInput(text),
        HostRequest::AddMidiOutput(text),
        HostRequest::Start,
        HostRequest::Status,
        HostRequest::Stop,
    ];
    let mut buf = [0; MAX_RECORD_BYTES];
    for request in requests {
        let len = encode_host_request(&request, &mut buf).unwrap();
        assert_eq!(decode_host_request(&buf[..len]), Ok(request));
        assert!(decode_client(&buf[..len]).is_err());
        for cut in 0..len {
            assert!(decode_host_request(&buf[..cut]).is_err());
        }
    }
    let replies = [
        HostReply::Hello {
            version: HOST_VERSION,
        },
        HostReply::Ok,
        HostReply::End,
        HostReply::Device {
            kind: DiscoverKind::MidiInputs,
            index: 0,
            name: text,
            default: true,
            channels: 2,
            sample_rate: 48000,
            min_buffer: 64,
            max_buffer: 1024,
        },
        HostReply::Warning(text),
        HostReply::InstanceName(text),
        HostReply::Error(text),
        HostReply::Stopped { midi_reset: true },
        HostReply::Ready(DriverStatus {
            kind: DriverKind::Dummy,
            state: DriverRunState::Running,
            sample_rate: 48000,
            buffer_frames: 128,
        }),
        HostReply::Status {
            frame: u64::MAX,
            callbacks: 12,
        },
    ];
    for reply in replies {
        let len = encode_host_reply(&reply, &mut buf).unwrap();
        assert_eq!(decode_host_reply(&buf[..len]), Ok(reply));
        assert!(decode_engine(&buf[..len]).is_err());
    }
}
#[shoop_test]
fn host_text_and_record_bounds_are_checked() {
    assert!(Text::new(&"a".repeat(MAX_HOST_TEXT_BYTES + 1)).is_none());
    assert!(Text::new(&"Δ".repeat(MAX_HOST_TEXT_BYTES / 2)).is_some());
    let clipped = Text::clipped(&"Δ".repeat(MAX_HOST_TEXT_BYTES));
    assert_eq!(clipped.as_str().len(), MAX_HOST_TEXT_BYTES);
    assert_eq!(
        encode_host_request(&HostRequest::Start, &mut [0; 1]),
        Err(CodecError::Overflow)
    );
    assert!(decode_host_request(&[4, 0, 0xf0, 255]).is_err());
    assert!(decode_host_reply(&[4, 0, 0xf1, 255]).is_err());
}

#[shoop_test]
fn jack_driver_identity_appends_a_tag_without_changing_existing_drivers() {
    let mut bytes = [0; MAX_RECORD_BYTES];
    for (kind, tag) in [
        (DriverKind::Dummy, 0),
        (DriverKind::Cpal, 1),
        (DriverKind::WebAudio, 2),
        (DriverKind::Jack, 3),
    ] {
        let reply = HostReply::Ready(DriverStatus {
            kind,
            state: DriverRunState::Running,
            sample_rate: 48000,
            buffer_frames: 128,
        });
        let len = encode_host_reply(&reply, &mut bytes).unwrap();
        assert_eq!(bytes[4], tag);
        assert_eq!(decode_host_reply(&bytes[..len]), Ok(reply));
    }
}
