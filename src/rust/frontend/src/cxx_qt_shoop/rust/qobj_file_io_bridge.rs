use common::logging::macros::*;
use cxx_qt_lib_shoop::qobject::AsQObject;
shoop_log_unit!("Frontend.FileIO");

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qlist.h");
        type QList_QString = cxx_qt_lib::QList<QString>;
        type QList_f32 = cxx_qt_lib::QList<f32>;
        type QList_QVariant = cxx_qt_lib::QList<QVariant>;

        include!("cxx-qt-lib/qvector.h");
        type QVector_f32 = cxx_qt_lib::QVector<f32>;
        type QVector_QVariant = cxx_qt_lib::QVector<QVariant>;

        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = cxx_qt_lib_shoop::qobject::QObject;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;
    }

    unsafe extern "RustQt" {
        #[qobject]
        type FileIO = super::FileIORust;

        #[qinvokable]
        pub fn wait_blocking(self: &FileIO, delay_ms: u64);

        #[qinvokable]
        pub fn get_current_directory(self: &FileIO) -> QString;

        #[qinvokable]
        pub fn write_file(self: &FileIO, file_name: QString, data: QString) -> bool;

        #[qinvokable]
        pub fn read_file(self: &FileIO, file_name: QString) -> QString;

        #[qinvokable]
        pub fn create_temporary_file(self: &FileIO) -> QString;

        #[qinvokable]
        pub fn generate_temporary_filename(self: &FileIO) -> QString;

        #[qinvokable]
        pub fn create_temporary_folder(self: &FileIO) -> QString;

        #[qinvokable]
        pub fn delete_recursive(self: &FileIO, path: QString) -> bool;

        #[qinvokable]
        pub fn delete_file(self: &FileIO, file_name: QString) -> bool;

        #[qinvokable]
        pub fn extract_tarfile(self: &FileIO, tarfile: QString, destination: QString) -> bool;

        #[qinvokable]
        pub fn make_tarfile(self: &FileIO, tarfile: QString, source: QString) -> bool;

        #[qinvokable]
        pub fn basename(self: &FileIO, path: QString) -> QString;

        #[qinvokable]
        pub fn is_absolute(self: &FileIO, path: QString) -> bool;

        #[qinvokable]
        pub fn realpath(self: &FileIO, path: QString) -> QString;

        #[qinvokable]
        pub fn exists(self: &FileIO, path: QString) -> bool;

        #[qinvokable]
        pub fn glob(self: &FileIO, pattern: QString) -> QList_QString;

        // data should be:
        // - QList<f32> (mono), or;
        // - QList<QList<f32>> (stereo).
        // more channels are not supported: cannot guarantee to save in a single file.
        #[qinvokable]
        pub fn save_data_to_soundfile(
            self: &FileIO,
            filename: QString,
            samplerate: i32,
            data: QVariant,
        ) -> bool;

        // returns an AsyncTask
        #[qinvokable]
        pub fn save_channel_to_midi_async(
            self: Pin<&mut FileIO>,
            filename: QString,
            samplerate: i32,
            channel: *mut QObject,
        ) -> *mut QObject;

        #[qinvokable]
        pub fn save_channel_to_midi(
            self: &FileIO,
            filename: QString,
            samplerate: i32,
            channel: *mut QObject,
        ) -> bool;

        // returns an AsyncTask
        #[qinvokable]
        pub fn load_midi_to_channels_async(
            self: Pin<&mut FileIO>,
            filename: QString,
            samplerate: i32,
            channels: QList_QVariant,
            maybe_set_n_preplay_samples: QVariant,
            maybe_set_start_offset: QVariant,
            maybe_update_loop_to_data_length: QVariant,
            ready_timeout_ms: i32,
        ) -> *mut QObject;

        #[qinvokable]
        pub fn load_midi_to_channels(
            self: &FileIO,
            filename: QString,
            samplerate: i32,
            channels: QList_QVariant,
            maybe_set_n_preplay_samples: QVariant,
            maybe_set_start_offset: QVariant,
            maybe_update_loop_to_data_length: QVariant,
            ready_timeout_ms: i32,
        ) -> bool;

        // returns an AsyncTask
        #[qinvokable]
        pub fn save_channels_to_soundfile_async(
            self: Pin<&mut FileIO>,
            filename: QString,
            samplerate: i32,
            channels: QList_QVariant,
        ) -> *mut QObject;

        #[qinvokable]
        pub fn save_channels_to_soundfile(
            self: &FileIO,
            filename: QString,
            samplerate: i32,
            channels: QList_QVariant,
        ) -> bool;

        // returns an AsyncTask
        #[qinvokable]
        pub fn load_soundfile_to_channels_async(
            self: Pin<&mut FileIO>,
            filename: QString,
            target_samplerate: i32,
            maybe_target_data_length: QVariant,
            channels_to_loop_channels: QList_QVariant,
            maybe_set_n_preplay_samples: QVariant,
            maybe_set_start_offset: QVariant,
            maybe_update_loop_to_data_length: QVariant,
            ready_timeout_ms: i32
        ) -> *mut QObject;

        #[qinvokable]
        pub fn load_soundfile_to_channels(
            self: &FileIO,
            filename: QString,
            target_samplerate: i32,
            maybe_target_data_length: QVariant,
            channels_to_loop_channels: QList_QVariant,
            maybe_set_n_preplay_samples: QVariant,
            maybe_set_start_offset: QVariant,
            maybe_update_loop_to_data_length: QVariant,
            ready_timeout_ms: i32
        ) -> bool;

        #[qinvokable]
        pub fn get_soundfile_info(self: &FileIO, filename: QString) -> QMap_QString_QVariant;

        #[qinvokable]
        pub fn get_soundfile_formats(self: &FileIO) -> QMap_QString_QVariant;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/make_unique.h");

        #[rust_name = "make_unique_fileio"]
        pub fn make_unique() -> UniquePtr<FileIO>;

        include!("cxx-qt-lib-shoop/register_qml_type.h");

        #[rust_name = "register_qml_singleton_file_io"]
        unsafe fn register_qml_singleton(
            inference_example: *mut FileIO,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );

        include!("cxx-qt-lib-shoop/qobject.h");

        #[rust_name = "from_qobject_ref_file_io"]
        unsafe fn fromQObjectRef(obj: &QObject, output: *mut *const FileIO);

        #[rust_name = "from_qobject_mut_file_io"]
        unsafe fn fromQObjectMut(obj: Pin<&mut QObject>, output: *mut *mut FileIO);

        #[rust_name = "file_io_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj: *mut FileIO) -> *mut QObject;

        #[rust_name = "file_io_qobject_from_ref"]
        fn qobjectFromRef(obj: &FileIO) -> &QObject;
    }
}

#[derive(Default)]
pub struct FileIORust {}

impl AsQObject for ffi::FileIO {
    unsafe fn mut_qobject_ptr(&mut self) -> *mut ffi::QObject {
        ffi::file_io_qobject_from_ptr(self as *mut Self)
    }

    unsafe fn ref_qobject_ptr(&self) -> *const ffi::QObject {
        ffi::file_io_qobject_from_ref(self) as *const ffi::QObject
    }
}
