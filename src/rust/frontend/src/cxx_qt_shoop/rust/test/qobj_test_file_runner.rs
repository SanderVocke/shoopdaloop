use crate::cxx_qt_shoop::test::qobj_test_file_runner_bridge::ffi::*;
use cxx_qt::CxxQtType;
use cxx_qt_lib_shoop::qobject::ffi::qobject_set_object_name;
use cxx_qt_lib_shoop::qobject::AsQObject;
use glob::glob;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Duration;

use common::logging::macros::*;
shoop_log_unit!("Frontend.TestFileRunner");

pub use crate::cxx_qt_shoop::test::qobj_test_file_runner_bridge::ffi::TestFileRunner;

impl TestFileRunner {
    pub unsafe fn make_raw(parent: *mut QObject) -> *mut TestFileRunner {
        let ptr = make_raw_test_runner(parent);
        ptr
    }

    fn maybe_run_next_test_file(mut self: Pin<&mut Self>) -> Result<(), anyhow::Error> {
        {
            let test_files = self.as_ref().test_files_to_run.clone();
            if test_files.len() == 0 {
                return Ok(());
            }
        }

        unsafe {
            let mut rust_mut = self.as_mut().rust_mut();
            let test_file = rust_mut.test_files_to_run.remove(0);

            let filename = test_file
                .file_name()
                .ok_or(anyhow::anyhow!("Unable to get filename"))?
                .to_string_lossy();

            println!();
            info!("===== Test file: {filename} =====");

            self.as_mut()
                .reload_qml(QString::from(test_file.to_string_lossy().to_string()));
        }

        Ok(())
    }

    pub fn start(
        mut self: Pin<&mut Self>,
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

            let glob_pattern = format!("{test_file_pattern}");
            debug!("Glob pattern: {glob_pattern}");
            glob(glob_pattern.as_str())?.try_for_each(|s| -> Result<(), anyhow::Error> {
                let s = s?;
                let s = s
                    .to_str()
                    .ok_or(anyhow::anyhow!("Cannot convert path to string"))?;
                all_test_files.push(PathBuf::from(s));
                Ok(())
            })?;

            debug!("Test files to run: {all_test_files:?}");

            {
                let mut rust_mut = self.as_mut().rust_mut();
                rust_mut.test_files_to_run = all_test_files.clone();
            }
            self.as_mut().maybe_run_next_test_file()?;

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

    pub fn on_testcase_done(self: Pin<&mut Self>) {
        debug!("testcase done");
        unsafe {
            self.unload_qml();
        }
    }

    pub unsafe fn on_qml_engine_destroyed(self: Pin<&mut TestFileRunner>) {
        debug!("QML engine destroyed");
        self.maybe_run_next_test_file();
    }
}
