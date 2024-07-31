#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "RustQt" {
        #[qobject]
        type FileIO = super::FileIORust;
    }

    unsafe extern "RustQt" {
        #[qsignal]
        fn startSavingFile(self : Pin<&mut FileIO>);

        #[qsignal]
        fn doneSavingFile(self : Pin<&mut FileIO>);

        #[qsignal]
        fn startLoadingFile(self : Pin<&mut FileIO>);

        #[qsignal]
        fn doneLoadingFile(self : Pin<&mut FileIO>);

        #[qinvokable]
        fn wait_blocking(self : &FileIO, delay_ms : u64);

        #[qinvokable]
        fn get_current_directory(self : &FileIO) -> QString;
    }

    unsafe extern "C++" {
        include!("cxx-shoop/make_unique.h");

        #[rust_name = "make_unique_fileio"]
        fn make_unique() -> UniquePtr<FileIO>;
    }

    unsafe extern "C++" {
        include!("cxx-shoop/is_called_from_qobj_thread.h");
        fn is_called_from_qobj_thread(object : &FileIO) -> bool;
    }
}

#[derive(Default)]
pub struct FileIORust {}


use qobject::*;
use std::thread;
use std::time::Duration;
use std::env;

#[allow(unreachable_code)]
impl FileIO {
    pub fn wait_blocking(self : &FileIO, delay_ms : u64) {
        thread::sleep(Duration::from_millis(delay_ms));
    }

    pub fn get_current_directory(self : &FileIO) -> QString {
        return match env::current_dir() {
            Ok(p) => match p.to_str() {
                    Some (pp) => return QString::from(pp),
                    None => QString::default(),
                },
            Err(_) => QString::default()
        }
    }
}


#[cfg(test)]
mod tests {
    use super::qobject::make_unique_fileio;

    #[test]
    fn test_dummy() {
        let obj = make_unique_fileio();
    }
}