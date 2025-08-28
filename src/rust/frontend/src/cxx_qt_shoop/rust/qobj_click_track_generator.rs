pub use crate::cxx_qt_shoop::qobj_click_track_generator_bridge::ffi::ClickTrackGenerator;
use crate::cxx_qt_shoop::qobj_click_track_generator_bridge::ffi::*;

pub fn register_qml_singleton(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_type_clicktrackgenerator(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}

impl ClickTrackGenerator {
    pub fn get_possible_clicks(self: &ClickTrackGenerator) -> QList_QString {
        self.generator.get_possible_clicks()
    }

    pub fn generate_audio(
        self: &ClickTrackGenerator,
        click_names: QList_QString,
        bpm: i32,
        n_beats: i32,
        alt_click_delay_percent: i32,
    ) -> QString {
        self.generator
            .generate_audio(click_names, bpm, n_beats, alt_click_delay_percent)
    }

    pub fn generate_midi(
        self: &ClickTrackGenerator,
        notes: QList_i32,
        channels: QList_i32,
        velocities: QList_i32,
        note_length: f32,
        bpm: i32,
        n_beats: i32,
        alt_click_delay_percent: i32,
    ) -> QString {
        self.generator.generate_midi(
            notes,
            channels,
            velocities,
            note_length,
            bpm,
            n_beats,
            alt_click_delay_percent,
        )
    }

    pub fn preview(self: &ClickTrackGenerator, wav_filename: QString) {
        self.generator.preview(wav_filename);
    }
}
