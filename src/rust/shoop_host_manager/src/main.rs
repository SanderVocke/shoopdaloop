#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    use clap::Parser;
    let args = shoop_host_manager::ManagerArgs::parse();
    if let Err(error) = shoop_host_manager::run(&args) {
        eprintln!("Host Manager failed: {error:#}");
        std::process::exit(1);
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {}
