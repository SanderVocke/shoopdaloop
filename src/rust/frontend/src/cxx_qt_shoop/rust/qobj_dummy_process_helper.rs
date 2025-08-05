use std::pin::Pin;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::thread;
use std::time::Duration;

use common::logging::macros::*;
shoop_log_unit!("Frontend.DummyProcessHelper");

use crate::cxx_qt_shoop::qobj_dummy_process_helper_bridge::ffi::*;
use crate::cxx_qt_shoop::qobj_signature_backend_wrapper;
use cxx_qt_lib_shoop::invokable;
use cxx_qt_lib_shoop::qobject::AsQObject;
use cxx_qt_lib_shoop::qvariant_qobject::qvariant_to_qobject_ptr;

impl DummyProcessHelper {
    pub fn start(mut self: Pin<&mut Self>) {
        let active = *self.as_mut().active();
        if active {
            error!("Cannot start dummy process helper: still running");
            return;
        }
        self.as_mut().set_active(true);

        let wait_start = self.wait_start().clone();
        let wait_interval = self.wait_interval().clone();
        let n_iters = self.n_iters().clone();
        let samples_per_iter = self.samples_per_iter().clone();

        debug!("Starting dummy process helper with {} iterations, {} samples per iteration, wait start {}s, wait interval {}s", n_iters, samples_per_iter, wait_start, wait_interval);

        let self_qobj_ptr;
        let backend_ptr;

        // TODO: find safer ways to do this
        unsafe {
            self_qobj_ptr = self.as_mut().pin_mut_qobject_ptr();
            let backend_variant = self.backend();
            backend_ptr = qvariant_to_qobject_ptr(&backend_variant).unwrap();
        }
        let self_aptr = AtomicPtr::new(self_qobj_ptr);
        let backend_aptr = AtomicPtr::new(backend_ptr);

        thread::spawn(move || {
            let self_thread_ptr = self_aptr.load(Ordering::SeqCst);
            let backend_thread_ptr = backend_aptr.load(Ordering::SeqCst);

            thread::sleep(Duration::from_secs_f32(wait_start));
            for _ in 0..n_iters {
                unsafe {
                    debug!("requesting {} frames", samples_per_iter);
                    qobj_signature_backend_wrapper::invoke_wait_process(
                        backend_thread_ptr.as_mut().unwrap(),
                        invokable::DIRECT_CONNECTION,
                    )
                    .unwrap();
                    qobj_signature_backend_wrapper::invoke_dummy_request_controlled_frames(
                        backend_thread_ptr.as_mut().unwrap(),
                        invokable::DIRECT_CONNECTION,
                        samples_per_iter,
                    )
                    .unwrap();
                }
                // Simulate backend processing
                thread::sleep(Duration::from_secs_f32(wait_interval));
            }

            unsafe {
                qobj_signature_backend_wrapper::invoke_wait_process(
                    backend_thread_ptr.as_mut().unwrap(),
                    invokable::DIRECT_CONNECTION,
                )
                .unwrap();
                debug!("Invoking finish");
                let _dummy: Result<(), _> = invokable::invoke(
                    self_thread_ptr.as_mut().unwrap(),
                    "finish()".to_string(),
                    invokable::DIRECT_CONNECTION,
                    &(),
                );
            }
        });
    }

    pub fn finish(self: Pin<&mut Self>) {
        debug!("Finished processing");
        self.set_active(false);
    }
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe { register_qml_type_dummy_process_helper(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp); }
}
