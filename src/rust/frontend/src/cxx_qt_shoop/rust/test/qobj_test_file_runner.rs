use crate::cxx_qt_shoop::test::qobj_test_file_runner_bridge::{ffi::*, TestCaseResults, TestFnResult, ResultStatus};
use cxx_qt::CxxQtType;
use cxx_qt_lib_shoop::qobject::ffi::qobject_set_object_name;
use cxx_qt_lib_shoop::qobject::{qobject_property_qvariant, AsQObject};
use cxx_qt_lib_shoop::qvariant_qvariantmap::{self, qvariant_as_qvariantmap};
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

    fn report_results(self: Pin<&mut Self>) -> bool {
        let mut failed_cases : Vec<String> = Vec::default();
        let mut skipped_cases : Vec<String> = Vec::default();
        let mut passed_cases : Vec<String> = Vec::default();

        self.test_results.iter().for_each(|testcase_results| {
            let testcase_name = &testcase_results.name;
            testcase_results.fn_results.iter().for_each(|fn_result : &TestFnResult| {
                let fn_name = &fn_result.name;
                let full_name = format!("{testcase_name}::{fn_name}");
                match fn_result.result {
                    ResultStatus::Pass => passed_cases.push(full_name),
                    ResultStatus::Skip => skipped_cases.push(full_name),
                    ResultStatus::Fail => failed_cases.push(full_name),
                }
            })
        });

        let n_failed = failed_cases.len() as i32;
        let n_passed = passed_cases.len() as i32;
        let n_skipped = skipped_cases.len() as i32;
        let n_total = n_failed + n_passed + n_skipped;

        let success : bool = n_failed == 0;

        let overall_result_readable = if success {
            "PASS"
        } else if n_skipped > 0 {
            "PASS WITH SKIPS"
        } else {
            "FAIL"
        };

        println!(r#"
========================================================
Test run finished. Overall result: {overall_result_readable}.
========================================================

Totals:
- Testcases: {n_total}
- Passed: {n_passed}
- Failed: {n_failed}
- Skipped: {n_skipped}
"#);

        if n_failed > 0 {
            println!("Failed cases:");
            for testcase in failed_cases.iter() {
                println!("- {testcase}");
            }
        }
        if n_skipped > 0 {
            println!("Skipped cases:");
            for testcase in skipped_cases.iter() {
                println!("- {testcase}");
            }
        }

        success
    }

    fn maybe_run_next_test_file(mut self: Pin<&mut Self>) {
        {
            let test_files = self.as_ref().test_files_to_run.clone();
            if test_files.len() == 0 {
                // No files left to test.
                let success = self.as_mut().report_results();
                unsafe { self.done(if success {0} else {1}); }
                return;
            }
        }

        unsafe {
            let mut rust_mut = self.as_mut().rust_mut();
            let test_file = rust_mut.test_files_to_run.remove(0);

            let filename = test_file
                .file_name()
                .unwrap()
                .to_string_lossy();

            println!();
            info!("===== Test file: {filename} =====");

            self.as_mut()
                .reload_qml(QString::from(test_file.to_string_lossy().to_string()));
        }
    }

    pub fn start(
        mut self: Pin<&mut Self>,
        test_file_pattern: QString,
        test_filter_pattern: QString,
        _application: *mut QObject,
        list_only: bool,
    ) -> bool {
        match (|| -> Result<(), anyhow::Error> {
            info!("Starting self-test runner.");

            let filter_pattern = match list_only {
                false => test_filter_pattern,
                true => QString::from("^$") //match nothing
            };
            debug!("Test function filter: {filter_pattern}");
            self.as_mut().set_test_filter_pattern(filter_pattern);

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
            self.as_mut().maybe_run_next_test_file();

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

    pub fn on_testcase_done(mut self: Pin<&mut Self>) {
        debug!("testcase done");

        unsafe {
            // Gather the test results from our QML runner and store them in our own Rust struct.
            let results : cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;
            {
                let runner = self.as_ref().testcase_runner;
                let results_variant = qobject_property_qvariant(& *runner, "testcase_results".to_string()).unwrap();
                results = qvariant_as_qvariantmap(&results_variant).unwrap();
            }

            {
                let mut rust_mut = self.as_mut().rust_mut();
                let our_results = &mut rust_mut.test_results;
                results.iter().try_for_each(|(testcase_name, testcase_content) : (&QString, &cxx_qt_lib::QVariant)| -> Result<(), anyhow::Error> {
                    let fn_results  = qvariant_as_qvariantmap(testcase_content).unwrap();
                    let mut testcase_results : TestCaseResults = TestCaseResults::default();
                    testcase_results.name = testcase_name.to_string();
                    fn_results.iter().try_for_each(|(testfn_name, testfn_content)| -> Result<(), anyhow::Error> {
                        let name = testfn_name.to_string();
                        let result = match testfn_content.value::<QString>().ok_or(anyhow::anyhow!("QVariant not convertible to QString"))?.to_string().as_str() {
                            "pass" => ResultStatus::Pass,
                            "fail" => ResultStatus::Fail,
                            "skip" => ResultStatus::Skip,
                            other => return Err(anyhow::anyhow!("Unknown test result {other}")),
                        };
                        debug!("found test function {name} result: {result:?}");
                        testcase_results.fn_results.push(TestFnResult { name: name, result: result });
                        Ok(())
                    })?;
                    our_results.push(testcase_results);
                    Ok(())
                }).unwrap();
            }
        }

        unsafe {
            // Unloading the QML engine will trigger running the next file or exiting.
            self.unload_qml();
        }
    }

    pub unsafe fn on_qml_engine_destroyed(self: Pin<&mut TestFileRunner>) {
        // When the QML engine gets destroyed, we either load the next file or
        // exit when done.
        debug!("QML engine destroyed");
        self.maybe_run_next_test_file();
    }
}
