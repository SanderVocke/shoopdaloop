#[cfg(test)]
mod test_init {
    use ctor::ctor;

    #[ctor]
    fn global_setup() {
        common::logging::init_logging().expect("Unable to initialize frontend logging for tests");
    }
}

#[cfg(not(feature = "prebuild"))]
pub mod init;

#[cfg(not(feature = "prebuild"))]
pub mod audio_power_pyramid;

#[cfg(not(feature = "prebuild"))]
pub mod cxx_qt_shoop;

#[cfg(not(feature = "prebuild"))]
mod loop_mode_helpers;

#[cfg(not(feature = "prebuild"))]
pub mod engine_update_thread;

#[cfg(not(feature = "prebuild"))]
pub mod test_results;

#[cfg(not(feature = "prebuild"))]
pub mod composite_loop_schedule;

#[cfg(not(feature = "prebuild"))]
pub mod loop_helpers;

#[cfg(not(feature = "prebuild"))]
pub mod profiling_report;

#[cfg(not(feature = "prebuild"))]
pub mod any_backend_port;

#[cfg(not(feature = "prebuild"))]
pub mod any_backend_channel;

#[cfg(not(feature = "prebuild"))]
pub mod midi_event_helpers;

#[cfg(not(feature = "prebuild"))]
pub mod references_qobject;

#[cfg(not(feature = "prebuild"))]
pub mod smf;

#[cfg(not(feature = "prebuild"))]
pub mod midi_io;

#[cfg(not(feature = "prebuild"))]
pub mod lua_engine;

#[cfg(not(feature = "prebuild"))]
pub mod lua_callback;

#[cfg(not(feature = "prebuild"))]
pub mod lua_qobject_callback;
