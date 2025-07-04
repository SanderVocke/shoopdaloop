#[cfg(test)]
mod test_init {
    use ctor::ctor;

    #[ctor]
    fn global_setup() {
        common::logging::init_logging().expect("Unable to initialize frontend logging for tests");
        crate::engine_update_thread::init();
    }
}

#[cfg(not(feature = "prebuild"))]
pub mod init;

#[cfg(not(feature = "prebuild"))]
pub mod audio_power_pyramid;

#[cfg(not(feature = "prebuild"))]
pub mod cxx_qt_shoop;

#[cfg(not(feature = "prebuild"))]
pub mod cxx_qt_lib_shoop;

#[cfg(not(feature = "prebuild"))]
mod tests;

#[cfg(not(feature = "prebuild"))]
mod loop_mode_helpers;

#[cfg(not(feature = "prebuild"))]
pub mod engine_update_thread;