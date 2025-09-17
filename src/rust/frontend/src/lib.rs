#[cfg(test)]
mod test_init {
    use ctor::ctor;

    #[ctor]
    fn global_setup() {
        common::logging::init_logging().expect("Unable to initialize frontend logging for tests");
    }
}

pub mod init;

pub mod audio_power_pyramid;

pub mod cxx_qt_shoop;

mod loop_mode_helpers;

pub mod engine_update_thread;

pub mod test_results;

pub mod composite_loop_schedule;

pub mod loop_helpers;

pub mod profiling_report;

pub mod any_backend_port;

pub mod any_backend_channel;

pub mod midi_event_helpers;

pub mod references_qobject;

pub mod smf;

pub mod midi_io;

pub mod lua_engine;

pub mod lua_callback;

pub mod lua_qobject_callback;

pub mod lua_conversions;
