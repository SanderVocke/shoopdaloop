use shoop_rs_macros::*;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "RustQt" {
        #[qobject]
        #[qproperty(u32, n_failures)]
        type TestObj = super::TestObjRust;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// An alias to the QString type
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        fn allow_own_thread(self: Pin<&mut TestObj>);

        #[qinvokable]
        fn allow_other_thread(self: Pin<&mut TestObj>);

        fn log_failure(self: Pin<&mut TestObj>, msg : &String);
    }

    unsafe extern "C++" {
        include!("cxx-shoop/make_unique.h");

        #[rust_name = "make_unique_testobj"]
        fn make_unique() -> UniquePtr<TestObj>;
    }

    unsafe extern "C++" {
        include!("cxx-shoop/is_called_from_qobj_thread.h");
        fn is_called_from_qobj_thread(object : Pin<&mut TestObj>) -> bool;
    }

    unsafe extern "C++" {
        include!("cxx-shoop/invoke_from_new_thread.h");
        fn invoke_from_new_thread(object : Pin<&mut TestObj>, method : QString) -> bool;
    }
}

#[derive(Default)]
pub struct TestObjRust {
    n_failures : u32,
}
use qobject::is_called_from_qobj_thread;
use std::pin::Pin;

#[allow(unreachable_code)]
impl qobject::TestObj {
    #[allow_own_thread_only(self.as_mut().log_failure)]
    pub fn allow_own_thread(self: Pin<&mut qobject::TestObj>) {}

    #[allow_other_thread_only(self.as_mut().log_failure)]
    pub fn allow_other_thread(self: Pin<&mut qobject::TestObj>) {}

    pub fn log_failure(self: Pin<&mut qobject::TestObj>, msg : &String) {}
}


#[cfg(test)]
mod tests {
    use super::qobject::*;

    #[test]
    fn test_allow_own_thread_allow() {
        let mut obj = make_unique_testobj();
        let acc = obj.as_mut().unwrap();
        // acc.n_failures = 0;
        acc.allow_own_thread();
        // assert_eq!(obj.as_mut().unwrap().n_failures, 0);
    }

    #[test]
    fn test_allow_own_thread_deny() {
        let mut obj = make_unique_testobj();
        let acc = obj.as_mut().unwrap();
        // acc.n_failures = 0;
        assert!(!invoke_from_new_thread(acc, QString::from("allowOwnThread")));
        // assert_eq!(acc.n_failures, 1);
    }

    #[test]
    fn test_allow_other_thread_deny() {
        let mut obj = make_unique_testobj();
        let acc = obj.as_mut().unwrap();
        // acc.n_failures = 0;
        acc.allow_other_thread();
        // assert_eq!(obj.as_mut().unwrap().n_failures, 1);
    }
}