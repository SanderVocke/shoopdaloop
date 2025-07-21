use crate::cxx_qt_shoop::test::qobj_test_runner_bridge::ffi::*;
use cxx_qt_lib_shoop::qobject::ffi::qobject_set_object_name;
use cxx_qt_lib_shoop::qobject::AsQObject;
use glob::glob;
use std::path::PathBuf;
use std::pin::Pin;

use common::logging::macros::*;
shoop_log_unit!("Frontend.TestRunner");

pub use crate::cxx_qt_shoop::test::qobj_test_runner_bridge::ffi::TestRunner;

impl TestRunner {
    pub unsafe fn make_raw(parent: *mut QObject) -> *mut TestRunner {
        let ptr = make_raw_test_runner(parent);
        let mut pin = std::pin::Pin::new_unchecked(&mut *ptr);
        qobject_set_object_name(
            pin.as_mut().pin_mut_qobject_ptr(),
            "ShoopTestRunner".to_string(),
        )
        .unwrap();
        ptr
    }

    pub fn start(
        self: Pin<&mut Self>,
        qml_files_path: QString,
        test_file_pattern: QString,
        test_filter_pattern: QString,
        application: *mut QObject,
        list_only: bool,
    ) -> bool {
        match (|| -> Result<(), anyhow::Error> {
            info!("Starting self-test runner.");

            let qml_files_path = qml_files_path.to_string();
            let test_file_pattern = test_file_pattern.to_string();
            let mut all_test_files: Vec<PathBuf> = Vec::default();

            glob(format!("{qml_files_path}/**/tst_*.qml").as_str())?.try_for_each(
                |s| -> Result<(), anyhow::Error> {
                    all_test_files.push(PathBuf::from(s?));
                    Ok(())
                },
            )?;

            debug!("Test files to run: {all_test_files:?}");

            Ok(())
        })() {
            Ok(()) => {
                return true;
            }
            Err(e) => {
                println!("Error running testcases: {}", e);
                return false;
            }
        }
    }
}
