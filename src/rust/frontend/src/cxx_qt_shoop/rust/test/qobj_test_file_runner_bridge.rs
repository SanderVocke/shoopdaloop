use std::path::PathBuf;

use cxx_qt_lib_shoop::qobject::AsQObject;

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = cxx_qt_lib_shoop::qobject::QObject;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qproperty(*mut QObject, testcase_runner)]
        #[qproperty(QString, test_filter_pattern)]
        type TestFileRunner = super::TestFileRunnerRust;

        #[qinvokable]
        pub unsafe fn start(
            self: Pin<&mut TestFileRunner>,
            test_file_pattern: QString,
            test_filter_pattern: QString,
            application: *mut QObject,
            list_only: bool,
        ) -> bool;

        #[qinvokable]
        pub unsafe fn on_testcase_done(self: Pin<&mut TestFileRunner>);

        #[qinvokable]
        pub unsafe fn on_qml_engine_destroyed(self: Pin<&mut TestFileRunner>);

        #[qsignal]
        pub unsafe fn reload_qml(self: Pin<&mut TestFileRunner>, qml_file: QString);

        #[qsignal]
        pub unsafe fn unload_qml(self: Pin<&mut TestFileRunner>);

        #[qsignal]
        pub unsafe fn process_app_events(self: Pin<&mut TestFileRunner>);

        #[qsignal]
        pub unsafe fn done(self: Pin<&mut TestFileRunner>, exit_code: i32);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/make_raw.h");
        #[rust_name = "make_raw_test_runner"]
        unsafe fn make_raw_with_one_arg(parent: *mut QObject) -> *mut TestFileRunner;

        include!("cxx-qt-lib-shoop/qobject.h");
        #[rust_name = "test_runner_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj: *mut TestFileRunner) -> *mut QObject;

        #[rust_name = "test_runner_qobject_from_ref"]
        fn qobjectFromRef(obj: &TestFileRunner) -> &QObject;
    }
}

impl AsQObject for ffi::TestFileRunner {
    unsafe fn mut_qobject_ptr(&mut self) -> *mut ffi::QObject {
        ffi::test_runner_qobject_from_ptr(self as *mut Self)
    }

    unsafe fn ref_qobject_ptr(&self) -> *const ffi::QObject {
        ffi::test_runner_qobject_from_ref(self) as *const ffi::QObject
    }
}

#[derive(Debug)]
pub enum ResultStatus {
    Pass,
    Fail,
    Skip,
}

pub struct TestFnResult {
    pub name: String,
    pub result: ResultStatus,
}

#[derive(Default)]
pub struct TestCaseResults {
    pub name: String,
    pub fn_results: Vec<TestFnResult>,
}
pub type TestResults = Vec<TestCaseResults>;

pub struct TestFileRunnerRust {
    pub running_testcase: Option<String>,
    pub current_testcase_done: bool,
    pub testcase_runner: *mut ffi::QObject,
    pub test_files_to_run: Vec<PathBuf>,
    pub test_files_ran: Vec<PathBuf>,
    pub test_filter_pattern: cxx_qt_lib::QString,
    pub test_results: TestResults,
}

impl Default for TestFileRunnerRust {
    fn default() -> TestFileRunnerRust {
        TestFileRunnerRust {
            running_testcase: None,
            current_testcase_done: false,
            testcase_runner: std::ptr::null_mut(),
            test_files_to_run: Vec::default(),
            test_files_ran: Vec::default(),
            test_filter_pattern: cxx_qt_lib::QString::from(""),
            test_results: TestResults::default(),
        }
    }
}
