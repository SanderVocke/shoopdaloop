use shoop_app_api::*;
use shoop_app_protocol::*;
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn driver_commands_round_trip_with_authority_and_refuse_old_versions() {
    for command in [
        audio_driver::Command::Discover(AudioDriverConfig::default()),
        audio_driver::Command::Request(AudioDriverConfig::Cpal(Default::default())),
        audio_driver::Command::Confirm {
            request_id: 7,
            accept: true,
        },
        audio_driver::Command::Confirm {
            request_id: 7,
            accept: false,
        },
        audio_driver::Command::RetryPersistence { request_id: 7 },
    ] {
        let message = WireMessage::Client(ClientMessage::Submit {
            authority: Authority {
                lease: LeaseId(1),
                client: ClientId(2),
                generation: ConnectionGeneration(3),
            },
            session_epoch: 1,
            operation: OperationId {
                client: ClientId(2),
                sequence: 4,
            },
            expected_revision: None,
            command: Command::AudioDriver(command),
        });
        let mut bytes = [0; shoop_transport::MAX_RECORD_BYTES];
        let used = codec::encode(&message, &mut bytes).unwrap();
        assert_eq!(codec::decode(&bytes[..used]).unwrap(), message);
        bytes[6..8].copy_from_slice(&(VERSION - 1).to_le_bytes());
        assert!(codec::decode(&bytes[..used]).is_err());
    }
}

#[shoop_test]
fn driver_profiles_are_bounded_before_admission() {
    let invalid = AudioDriverConfig::Cpal(CpalAudioDriverConfig {
        output_device: "x".repeat(129),
        ..Default::default()
    });
    assert!(!Command::AudioDriver(audio_driver::Command::Request(invalid)).bounded());
    assert!(!Command::AudioDriver(audio_driver::Command::Confirm {
        request_id: 0,
        accept: true
    })
    .bounded());
    assert!(
        !Command::AudioDriver(audio_driver::Command::Request(AudioDriverConfig::WebAudio))
            .bounded()
    );
}
