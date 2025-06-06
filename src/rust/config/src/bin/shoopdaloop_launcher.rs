use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, exit};
use std::io;
use common::logging::macros::*;
use config::config::ShoopConfig;
use std::collections::HashMap;

shoop_log_unit!("Main");

fn main() -> io::Result<()> {
    let mut subprocess_env : HashMap<String, String> = env::vars().collect();

    common::init().unwrap();

    let config = ShoopConfig::load().expect("Could not load config");

    if config.dynlibpaths.len() > 0 {
        let dynlibpaths_string = config.dynlibpaths.join(common::fs::PATH_LIST_SEPARATOR);

        let runtime_link_path_var =
        if cfg!(target_os = "windows") { "PATH" }
        else if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" }
        else { "LD_LIBRARY_PATH" };

        debug!("Adding to {}: {}", 
            runtime_link_path_var, dynlibpaths_string);

        let old_runtime_link_paths = env::var(runtime_link_path_var)
            .unwrap_or_default();
        let new_runtime_link_paths = format!(
            "{}{}{}",
            dynlibpaths_string,
            common::fs::PATH_LIST_SEPARATOR,
            old_runtime_link_paths
        );
        // Set the modified runtime link paths environment variable.
        subprocess_env.insert(runtime_link_path_var.to_string(), new_runtime_link_paths);
    } else {
        debug!("launcher: no additional library paths to add.");
    }
    
    // Get the launcher's directory.
    let launcher_path = env::current_exe().expect("Failed to get launcher path");
    let launcher_dir = launcher_path
        .parent()
        .expect("Failed to get launcher directory");

    let shoopdaloop_executable = launcher_dir.join("shoopdaloop");

    // Collect all arguments passed to the launcher and pass them to the executable.
    let args: Vec<String> = env::args().skip(1).collect();

    // Prepare to launch the executable in the modified environment with arguments.
    let mut command = Command::new(shoopdaloop_executable);
    command.args(&args);
    for (key, value) in subprocess_env {
        debug!("Set env {}: {}", key, value);
        command.env(key, value);
    }
    info!("Launching ShoopDaLoop.");
    let status = command.spawn()?.wait()?;

    // Exit with the same code as the launched executable.
    exit(status.code().unwrap_or_default());
}
