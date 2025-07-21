use std::collections::HashMap;

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
        type TestRunner = super::TestRunnerRust;

        #[qinvokable]
        pub unsafe fn start(
            self: Pin<&mut TestRunner>,
            qml_files_path: QString,
            test_file_pattern: QString,
            test_filter_pattern: QString,
            application: *mut QObject,
            list_only: bool,
        ) -> bool;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/make_raw.h");
        #[rust_name = "make_raw_test_runner"]
        unsafe fn make_raw_with_one_arg(parent: *mut QObject) -> *mut TestRunner;

        include!("cxx-qt-lib-shoop/qobject.h");
        #[rust_name = "test_runner_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj: *mut TestRunner) -> *mut QObject;

        #[rust_name = "test_runner_qobject_from_ref"]
        fn qobjectFromRef(obj: &TestRunner) -> &QObject;
    }
}

impl AsQObject for ffi::TestRunner {
    unsafe fn mut_qobject_ptr(&mut self) -> *mut ffi::QObject {
        ffi::test_runner_qobject_from_ptr(self as *mut Self)
    }

    unsafe fn ref_qobject_ptr(&self) -> *const ffi::QObject {
        ffi::test_runner_qobject_from_ref(self) as *const ffi::QObject
    }
}

#[derive(Default)]
pub struct TestRunnerRust {
    pub running_testcase: Option<String>,
    pub ran_testcase_results: HashMap<String, Result<(), String>>,
}
