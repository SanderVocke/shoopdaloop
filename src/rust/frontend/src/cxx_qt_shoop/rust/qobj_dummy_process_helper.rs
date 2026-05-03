use std::pin::Pin;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::thread;
use std::time::Duration;

use common::logging::macros::*;
shoop_log_unit!("Frontend.DummyProcessHelper");

use crate::cxx_qt_shoop::qobj_dummy_process_helper_bridge::ffi::*;
use cxx_qt::CxxQtType;
use cxx_qt_lib_shoop::invokable;
use cxx_qt_lib_shoop::qobject::AsQObject;
use cxx_qt_lib_shoop::qvariant_helpers::qvariant_to_qobject_ptr;

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

        if common::tracing_helpers::is_tracing_enabled()
            && tracy_client::Client::running().is_some()
        {
            let mut rust = self.as_mut().rust_mut();
            let identifier = "DummyProcessHelper";
            rust.plotter_active.plot(1.0, identifier);
            rust.plotter_n_iters.plot(n_iters as f64, identifier);
            rust.plotter_samples_per_iter
                .plot(samples_per_iter as f64, identifier);
        }

        debug!("Starting dummy process helper with {} iterations, {} samples per iteration, wait start {}s, wait interval {}s", n_iters, samples_per_iter, wait_start, wait_interval);

        let self_qobj_ptr;
        let backend_ptr;

        // TODO: find safer ways to do this
        unsafe {
            self_qobj_ptr = self.as_mut().pin_mut_qobject_ptr();
            let backend_variant = self.backend();
            // Fallback to null if conversion fails. checking null later or assuming safe if we let it happen?
            // Better to check.
            backend_ptr = qvariant_to_qobject_ptr(&backend_variant).unwrap_or(std::ptr::null_mut());
        }

        if backend_ptr.is_null() {
            error!("Cannot start dummy process helper: backend is null or invalid");
            self.set_active(false);
            return;
        }

        let self_aptr = AtomicPtr::new(self_qobj_ptr);
        let backend_aptr = AtomicPtr::new(backend_ptr);

        thread::spawn(move || {
            let self_thread_ptr = self_aptr.load(Ordering::SeqCst);
            let backend_thread_ptr = backend_aptr.load(Ordering::SeqCst);

            if self_thread_ptr.is_null() || backend_thread_ptr.is_null() {
                error!(
                    "DummyProcessHelper thread: pointers are null (self: {:?}, backend: {:?})",
                    self_thread_ptr, backend_thread_ptr
                );
                return;
            }

            thread::sleep(Duration::from_secs_f32(wait_start));
            for iter in 0..n_iters {
                unsafe {
                    if common::tracing_helpers::is_tracing_enabled()
                        && tracy_client::Client::running().is_some()
                    {
                        if let Some(self_obj) = self_thread_ptr.as_mut() {
                            let mut rust = self_obj.rust_mut();
                            rust.plotter_iteration
                                .plot(iter as f64, "DummyProcessHelper");
                        }
                    }
                    debug!("requesting {} frames", samples_per_iter);
                    if let Some(backend) = backend_thread_ptr.as_mut() {
                        if let Err(e) = invokable::invoke::<_, (), _>(
                            backend,
                            "wait_process()",
                            invokable::DIRECT_CONNECTION,
                            &(),
                        ) {
                            error!("DummyProcessHelper: wait_process failed: {e}");
                        }

                        if let Err(e) = invokable::invoke::<_, (), _>(
                            backend,
                            "dummy_request_controlled_frames(::std::int32_t)",
                            invokable::DIRECT_CONNECTION,
                            &(samples_per_iter),
                        ) {
                            error!(
                                "DummyProcessHelper: dummy_request_controlled_frames failed: {e}"
                            );
                        }
                    } else {
                        error!("DummyProcessHelper: backend pointer became null during loop");
                        break;
                    }
                }
                // Simulate backend processing
                thread::sleep(Duration::from_secs_f32(wait_interval));
            }

            unsafe {
                if let Some(backend) = backend_thread_ptr.as_mut() {
                    if let Err(e) = invokable::invoke::<_, (), _>(
                        backend,
                        "wait_process()",
                        invokable::DIRECT_CONNECTION,
                        &(),
                    ) {
                        error!("DummyProcessHelper: wait_process (final) failed: {e}");
                    }
                }

                debug!("Invoking finish");
                if let Some(this) = self_thread_ptr.as_mut() {
                    let _dummy: Result<(), _> =
                        invokable::invoke(this, "finish()", invokable::DIRECT_CONNECTION, &());
                } else {
                    error!("DummyProcessHelper: self pointer became null at finish");
                }
            }
        });
    }

    pub fn finish(mut self: Pin<&mut Self>) {
        debug!("Finished processing");
        if common::tracing_helpers::is_tracing_enabled()
            && tracy_client::Client::running().is_some()
        {
            let mut rust = self.as_mut().rust_mut();
            let identifier = "DummyProcessHelper";
            rust.plotter_active.plot(0.0, identifier);
        }
        self.set_active(false);
    }
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_type_dummy_process_helper(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}
