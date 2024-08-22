#[cfg(test)]
mod test_init {
    use ctor::ctor;

    #[ctor]
    fn global_setup() {
        crate::logging::init_logging().expect("Unable to initialize shoop_rs_frontend logging for tests");
    }
}

pub mod init;

pub mod audio_power_pyramid;
pub mod logging;

pub mod cxx_qt_shoop;
pub mod cxx_qt_lib_shoop;

pub mod shoop_rs_macros_tests;

mod tests;