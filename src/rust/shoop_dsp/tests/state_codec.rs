//! Persisted state envelopes remain usable by a client without either DSP build.
#![cfg(any(feature = "builtin-fx", feature = "oxisynth"))]
use shoop_processor_protocol::{state::State, Kind};
use shoop_wasm_test_support::shoop_test;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[cfg(feature = "builtin-fx")]
#[shoop_test]
fn portable_fx_envelopes_preserve_reference_versions_types_and_exact_float_bits() {
    use shoop_dsp::builtin_fx::*;
    for encoded in ["shoop-builtin-fx:1:0", "shoop-builtin-fx:1:1"] {
        let reference = BuiltInFxState::decode(encoded).unwrap();
        assert_eq!(
            State::decode(Kind::BuiltInFx, encoded).unwrap().encode(),
            reference.encode()
        );
    }
    for drive_type in [
        DriveType::Saturation,
        DriveType::Overdrive,
        DriveType::Distortion,
        DriveType::Fuzz,
    ] {
        for modulation_type in [
            ModulationType::Tremolo,
            ModulationType::Flanger,
            ModulationType::Phaser,
        ] {
            for reverb_type in [ReverbType::Room, ReverbType::Hall, ReverbType::Plate] {
                let state = BuiltInFxState {
                    drive_type,
                    modulation_type,
                    reverb_type,
                    compressor_enabled: true,
                    compressor_threshold_db: -37.125,
                    drive_enabled: true,
                    eq_enabled: true,
                    eq_low_db: -0.,
                    chorus_enabled: true,
                    modulation_enabled: true,
                    reverb_enabled: false,
                    modulation_feedback: -0.7,
                    ..Default::default()
                };
                let encoded = state.encode();
                let portable = State::decode(Kind::BuiltInFx, &encoded).unwrap();
                assert_eq!(portable.encode(), encoded);
                assert_eq!(BuiltInFxState::decode(&portable.encode()).unwrap(), state);
                assert_eq!(portable.values[9].to_bits(), (-0f32).to_bits());
            }
        }
    }
    let mut invalid = State::new(Kind::BuiltInFx).unwrap();
    for (index, value) in [(0, 1.), (2, 0.), (19, 1.), (22, f32::NAN)] {
        let old = invalid.values[index];
        invalid.values[index] = value;
        assert!(State::decode(Kind::BuiltInFx, &invalid.encode()).is_err());
        assert!(BuiltInFxState::decode(&invalid.encode()).is_err());
        invalid.values[index] = old;
    }
}
#[cfg(feature = "oxisynth")]
#[shoop_test]
fn portable_synth_envelopes_use_host_presets_without_loading_a_soundfont() {
    use shoop_dsp::oxisynth::*;
    for preset in available_presets() {
        let state = OxiSynthState {
            preset: preset.id,
            reverb_send: 0.375,
            chorus_send: 0.625,
        };
        let reference = OxiSynthControlState::from_state(state).unwrap();
        let encoded = reference.encode();
        let portable = State::decode(Kind::OxiSynth, &encoded).unwrap();
        assert_eq!(portable.encode(), encoded);
        assert_eq!(
            OxiSynthControlState::from_encoded(&portable.encode())
                .unwrap()
                .state(),
            state
        );
    }
    for invalid in [
        "shoop-oxisynth:2:other:0:0:00000000:00000000",
        "shoop-oxisynth:2:timgm6mb:00:0:00000000:00000000",
        "shoop-oxisynth:2:timgm6mb:0:128:00000000:00000000",
        "shoop-oxisynth:2:timgm6mb:0:0:7fc00000:00000000",
    ] {
        assert!(State::decode(Kind::OxiSynth, invalid).is_err());
        assert!(OxiSynthControlState::from_encoded(invalid).is_err());
    }
}
