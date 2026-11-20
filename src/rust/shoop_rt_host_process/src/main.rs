#[cfg(not(target_arch = "wasm32"))]
fn main() {
    if let Err(error) = run() {
        eprintln!("engine process: {error:#}");
        std::process::exit(1);
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn run() -> anyhow::Result<()> {
    #[cfg(feature = "carla-worker")]
    {
        use shoop_carla::native;
        if let Some(options) = shoop_carla::child::worker_options(std::env::args_os())? {
            return shoop_carla::child::run(options);
        }
        match std::env::args_os()
            .nth(1)
            .as_deref()
            .and_then(std::ffi::OsStr::to_str)
        {
            Some("--probe-carla-native") => {
                native::smoke_test_carla_runtime()?;
                println!(
                    "Carla Native runtime at {}",
                    native::carla_runtime_path()?.display()
                );
                return Ok(());
            }
            Some("--probe-carla-native-ui") => return native::smoke_test_carla_ui(),
            _ => {}
        }
    }
    shoop_rt_host_process::run()
}
#[cfg(target_arch = "wasm32")]
fn main() {}
