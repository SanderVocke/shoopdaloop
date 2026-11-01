#[cfg(not(target_arch = "wasm32"))]
fn main() {
    if let Err(error) = shoop_rt_host_process::run() {
        eprintln!("engine process: {error:#}");
        std::process::exit(1);
    }
}
#[cfg(target_arch = "wasm32")]
fn main() {}
