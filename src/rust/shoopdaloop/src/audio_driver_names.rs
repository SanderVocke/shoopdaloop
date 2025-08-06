use backend_bindings::AudioDriverType;
use std::iter::*;

pub fn get_audio_driver_name(driver_type: AudioDriverType) -> &'static str {
    match driver_type {
        AudioDriverType::Dummy => "dummy",
        AudioDriverType::Jack => "jack",
        AudioDriverType::JackTest => "jack_test",
    }
}

pub fn get_audio_driver_from_name(driver_name: &str) -> AudioDriverType {
    match driver_name {
        "dummy" => AudioDriverType::Dummy,
        "jack" => AudioDriverType::Jack,
        "jack_test" => AudioDriverType::JackTest,
        _ => panic!("Unknown audio driver name: {}", driver_name),
    }
}

pub fn all_audio_driver_types() -> impl Iterator<Item = AudioDriverType> {
    once(AudioDriverType::Dummy)
        .chain(once(AudioDriverType::Jack))
        .chain(once(AudioDriverType::JackTest))
}
