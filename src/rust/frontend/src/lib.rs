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
pub mod cxx_qt_lib_shoop;

#[cfg(not(feature = "prebuild"))]
mod tests;