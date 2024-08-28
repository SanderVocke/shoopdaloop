use crate::logging::macros::*;
shoop_log_unit!("Frontend.FileIO");

#[cxx_qt::bridge(cxx_file_stem="qobj_file_io")]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qlist.h");
        type QList_QString = cxx_qt_lib::QList<QString>;
    }

    unsafe extern "RustQt" {
        #[qobject]
        type FileIO = super::FileIORust;
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        fn wait_blocking(self : &FileIO, delay_ms : u64);

        #[qinvokable]
        fn get_current_directory(self : &FileIO) -> QString;

        #[qinvokable]
        fn write_file(self : &FileIO, file_name : QString, data : QString) -> bool;

        #[qinvokable]
        fn read_file(self : &FileIO, file_name : QString) -> QString;

        #[qinvokable]
        fn create_temporary_file(self : &FileIO) -> QString;

        #[qinvokable]
        fn generate_temporary_filename(self : &FileIO) -> QString;

        #[qinvokable]
        fn create_temporary_folder(self : &FileIO) -> QString;

        #[qinvokable]
        fn delete_recursive(self : &FileIO, path : QString) -> bool;

        #[qinvokable]
        fn delete_file(self : &FileIO, file_name : QString) -> bool;

        #[qinvokable]
        fn extract_tarfile(self : &FileIO, tarfile : QString, destination : QString) -> bool;

        #[qinvokable]
        fn make_tarfile(self : &FileIO, tarfile : QString, source : QString) -> bool;

        #[qinvokable]
        fn basename(self : &FileIO, path : QString) -> QString;

        #[qinvokable]
        fn is_absolute(self : &FileIO, path : QString) -> bool;

        #[qinvokable]
        fn realpath(self : &FileIO, path : QString) -> QString;

        #[qinvokable]
        fn exists(self : &FileIO, path : QString) -> bool;

        #[qinvokable]
        fn glob(self : &FileIO, pattern : QString) -> QList_QString;
    }

    unsafe extern "C++" {
        include!("cxx-qt-shoop/make_unique.h");

        #[rust_name = "make_unique_fileio"]
        fn make_unique() -> UniquePtr<FileIO>;
    }

    unsafe extern "C++" {
        include!("cxx-qt-shoop/register_qml_type.h");

        #[rust_name = "register_qml_singleton_file_io"]
        fn register_qml_singleton(inference_example: &FileIO,
                                  module_name : &mut String,
                                  version_major : i64, version_minor : i64,
                                  type_name : &mut String);
    }
}

#[derive(Default)]
pub struct FileIORust {}