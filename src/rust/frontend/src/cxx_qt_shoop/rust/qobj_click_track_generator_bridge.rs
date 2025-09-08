#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qlist.h");
        type QList_i32 = cxx_qt_lib::QList<i32>;
        type QList_f32 = cxx_qt_lib::QList<f32>;
        type QList_QString = cxx_qt_lib::QList<QString>;
        type QList_QVariant = cxx_qt_lib::QList<cxx_qt_lib::QVariant>;
    }

    unsafe extern "RustQt" {
        #[qobject]
        type ClickTrackGenerator = super::ClickTrackGeneratorRust;

        #[qinvokable]
        pub fn get_possible_clicks(self: &ClickTrackGenerator) -> QList_QString;

        #[qinvokable]
        pub fn generate_audio(
            self: &ClickTrackGenerator,
            click_names: QList_QString,
            bpm: i32,
            n_beats: i32,
            alt_click_delay_percent: i32,
            sample_rate : usize,
        ) -> QList_f32;

        #[qinvokable]
        pub fn generate_audio_into_channels(
            self: &ClickTrackGenerator,
            click_names: QList_QString,
            bpm: i32,
            n_beats: i32,
            alt_click_delay_percent: i32,
            sample_rate : usize,
            channels: QList_QVariant
        ) -> i32;

        #[qinvokable]
        pub fn generate_midi(
            self: &ClickTrackGenerator,
            notes: QList_i32,
            channels: QList_i32,
            velocities: QList_i32,
            note_length: f32,
            bpm: i32,
            n_beats: i32,
            alt_click_delay_percent: i32,
            sample_rate: i32,
        ) -> QString;

        #[qinvokable]
        pub fn preview_audio(
            self: &ClickTrackGenerator,
            click_names: QList_QString,
            bpm: i32,
            n_beats: i32,
            alt_click_delay_percent: i32,
            sample_rate: i32,
        );
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/make_unique.h");

        #[rust_name = "make_unique_clicktrackgenerator"]
        fn make_unique() -> UniquePtr<ClickTrackGenerator>;

        include!("cxx-qt-lib-shoop/register_qml_type.h");

        #[rust_name = "register_qml_type_clicktrackgenerator"]
        unsafe fn register_qml_singleton(
            inference_example: *mut ClickTrackGenerator,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );
    }
}

pub struct ClickTrackGeneratorRust {
}

impl Default for ClickTrackGeneratorRust {
    fn default() -> Self {
        Self {
        }
    }
}
