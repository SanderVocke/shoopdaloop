#[cfg(test)]
mod test_init {
    use ctor::ctor;

    #[ctor]
    fn global_setup() {
        crate::logging::init_logging().expect("Unable to initialize frontend logging for tests");
    }
}

pub mod init;

pub mod audio_power_pyramid;

pub mod cxx_qt_shoop;
pub mod cxx_qt_lib_shoop;

mod tests;