use shoop_rs_macros::*;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "RustQt" {
        #[qobject]
        type TestObj = super::TestObjRust;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// An alias to the QString type
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        fn own_thread_only_mut(self: Pin<&mut TestObj>);

        #[qinvokable]
        fn other_thread_only_mut(self: Pin<&mut TestObj>);
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
}

#[derive(Default)]
pub struct TestObjRust {}
use qobject::is_called_from_qobj_thread;
use std::pin::Pin;

#[allow(unreachable_code)]
impl qobject::TestObj {
    #[deny_calls_not_on_object_thread]
    pub fn own_thread_only_mut(self: Pin<&mut qobject::TestObj>) {}

    #[deny_calls_on_object_thread]
    pub fn other_thread_only_mut(self: Pin<&mut qobject::TestObj>) {}
}


#[cfg(test)]
mod tests {
    use super::qobject::*;
    use std::thread;
    use std::mem;
    use std::panic;
    use std::sync::{Arc, Mutex};
    use std::pin::Pin;

    struct TransferPtrUnsafe {
        ptr: *mut TestObj,
    }
    unsafe impl Send for TransferPtrUnsafe {}
    unsafe impl Sync for TransferPtrUnsafe {}

    macro_rules! assert_call_ptr_from_other_thread {
        ($ptr:expr, $fn:tt, $should_panic:expr) => {
            // Some unsafe acrobatics to transfer our object reference
            // to another thread
            unsafe {
                let transfer = TransferPtrUnsafe { ptr : $ptr };
                let arc = Arc::new(Mutex::new(transfer));
                let handle = thread::spawn(move || {
                    let result = panic::catch_unwind(|| {
                        let unsafe_mut : &mut TestObj = mem::transmute(arc.lock().unwrap().ptr);
                        let unsafe_pin : Pin<&mut TestObj> = Pin::new_unchecked(unsafe_mut);
                        unsafe_pin.$fn();
                    });
                    let panicked = matches!(result, Err(_));
                    let should_panic = $should_panic;
                    assert_eq!(panicked, should_panic);
                });
                let result = handle.join();
                assert!(!matches!(result, Err(_)));
            }
        }
    }

    macro_rules! test_variant {
        ($fn:tt, $do_on_thread:expr, $should_panic:expr) => {
            if $do_on_thread {
                let mut obj = make_unique_testobj();
                let acc = obj.as_mut().unwrap();
                assert_call_ptr_from_other_thread!(acc.get_unchecked_mut(), $fn, $should_panic);
            } else {
                let result = panic::catch_unwind(|| {
                    let mut obj = make_unique_testobj();
                    let acc = obj.as_mut().unwrap();
                    acc.$fn();
                });
                let panicked = matches!(result, Err(_));
                let should_panic = $should_panic;
                assert_eq!(panicked, should_panic);
            }
        }
    }

    #[test]
    fn test_own_thread_only_mut_allow() { test_variant!(own_thread_only_mut, false, false); }

    #[test]
    fn test_own_thread_only_mut_deny() { test_variant!(own_thread_only_mut, true, true); }

    #[test]
    fn test_other_thread_only_mut_allow() { test_variant!(other_thread_only_mut, true, false); }

    #[test]
    fn test_other_thread_only_mut_deny() { test_variant!(other_thread_only_mut, false, true); }
}