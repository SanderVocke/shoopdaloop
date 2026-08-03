use crate::cxx_qt_shoop::qobj_frontend_refresh_bridge::ffi::*;
use crate::cxx_qt_shoop::qobj_frontend_refresh_bridge::FrontendRefresh;
use common::logging::macros::*;
use cxx_qt::CxxQtType;
use cxx_qt_lib_shoop::connection_types;
use cxx_qt_lib_shoop::invokable;
use cxx_qt_lib_shoop::qobject::AsQObject;
use std::pin::Pin;

shoop_log_unit!("Frontend.Refresh");

impl FrontendRefresh {
    pub fn initialize_impl(mut self: Pin<&mut Self>) {
        let _span = tracing::info_span!("frontend.refresh.initialize").entered();
        unsafe {
            let self_qobject = self.as_mut().pin_mut_qobject_ptr();
            let timer_ptr = QTimer::make_raw_with_parent(self_qobject);
            let mut timer = Pin::new_unchecked(&mut *timer_ptr);
            timer.as_mut().set_interval(self.fallback_interval_ms);
            timer.as_mut().set_single_shot(false);
            if let Err(error) = timer.as_mut().connect_timeout(self_qobject, "timer_tick()") {
                error!("Failed to connect frontend refresh timer: {error}");
            }
            timer.as_mut().start();
            self.as_mut().rust_mut().fallback_timer = timer_ptr;
        }
    }

    pub fn request_refresh(mut self: Pin<&mut Self>) {
        let _span = tracing::trace_span!(
            "frontend.refresh.request",
            already_queued = self.refresh_queued
        )
        .entered();
        if self.refresh_queued {
            return;
        }
        {
            let mut rust = self.as_mut().rust_mut();
            rust.refresh_queued = true;
            rust.refresh_requested_at = Some(std::time::Instant::now());
        }
        unsafe {
            let self_qobject = self.as_mut().pin_mut_qobject_ptr();
            if let Err(error) = invokable::invoke::<_, (), _>(
                &mut *self_qobject,
                "do_queued_refresh()",
                connection_types::QUEUED_CONNECTION,
                &(),
            ) {
                error!("Failed to queue frontend refresh: {error}");
                self.as_mut().rust_mut().refresh_queued = false;
                self.as_mut().refresh_now();
            }
        }
    }

    pub fn do_queued_refresh(mut self: Pin<&mut Self>) {
        let delay_us = self
            .as_mut()
            .rust_mut()
            .refresh_requested_at
            .take()
            .map(|requested| requested.elapsed().as_micros() as u64)
            .unwrap_or(0);
        let _span = tracing::debug_span!("frontend.refresh.queued", delay_us).entered();
        {
            let mut rust = self.as_mut().rust_mut();
            rust.refresh_queued = false;
            rust.queue_delay_plotter
                .plot(delay_us as f64, "FrontendRefresh");
        }
        self.as_mut().refresh_now();
    }

    pub fn refresh_now(mut self: Pin<&mut Self>) {
        let _span = tracing::debug_span!("frontend.refresh.run").entered();
        self.as_mut().refresh();
        if common::tracing_helpers::is_tracing_enabled() {
            if let Some(client) = tracy_client::Client::running() {
                client.secondary_frame_mark(tracy_client::frame_name!("frontend.refresh"));
            }
        }
    }

    pub fn timer_tick(self: Pin<&mut Self>) {
        let _span = tracing::trace_span!("frontend.refresh.fallback_timer").entered();
        self.request_refresh();
    }

    pub fn set_fallback_interval_ms(mut self: Pin<&mut Self>, interval_ms: i32) {
        let interval_ms = interval_ms.max(1);
        self.as_mut().rust_mut().fallback_interval_ms = interval_ms;
        let timer = self.fallback_timer;
        if timer.is_null() {
            return;
        }
        unsafe {
            Pin::new_unchecked(&mut *timer).set_interval(interval_ms);
        }
    }

    pub fn make_unique() -> cxx::UniquePtr<FrontendRefresh> {
        make_unique_frontend_refresh()
    }
}
