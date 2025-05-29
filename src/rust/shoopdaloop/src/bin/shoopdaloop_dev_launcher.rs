use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, exit};
use std::io;
use common::logging::macros::*;
shoop_log_unit!("Main");

const RUNTIME_LINK_PATHS_STR : &str = env!("SHOOP_RUNTIME_LINK_PATHS");
const SRC_DIR : &str = env!("CARGO_MANIFEST_DIR");

fn main() -> io::Result<()> {
    common::init().unwrap();
    
    // // Get the launcher's directory.
    let launcher_path = env::current_exe().expect("Failed to get launcher path");
    let launcher_dir = launcher_path
        .parent()
        .expect("Failed to get launcher directory");

    // // Load and parse paths from the paths file
    // let paths_file_path = launcher_dir.join(PATHS_FILE);
    // let paths_content = fs::read_to_string(&paths_file_path)
    //     .expect("Failed to read paths file");

    // let executable_file_path = launcher_dir.join(EXECUTABLE_FILE);
    // let executable = fs::read_to_string(&executable_file_path)
    //     .expect("Failed to read executable file");

    // // Collect paths and prepare to add them to PATH.
    // let additional_paths: Vec<PathBuf> = paths_content
    //     .lines()
    //     .map(|line| launcher_dir.join(line.trim()))
    //     .collect();

    // // Get the existing PATH and append our additional paths.
    // let current_path = env::var("PATH").unwrap_or_default();
    // debug!("environment PATH: {}", env::var("PATH").unwrap_or_default());
    // let new_path = format!(
    //     "{};{}",
    //     env::join_paths(additional_paths.iter().map(|p| p.as_path()))
    //        .expect("Failed to join paths")
    //        .to_str().expect("Unable to convert new paths"),
    //     current_path
    // );

    // // Set the modified PATH environment variable.
    // env::set_var("PATH", &new_path);

    // debug!("new PATH: {}", env::var("PATH").unwrap_or_default());

    let normalize_path = |path: PathBuf| -> PathBuf {
        PathBuf::from(std::fs::canonicalize(path).unwrap().to_str().unwrap().trim_start_matches(r"\\?\"))
    };

    let runtime_link_path_var =
        if cfg!(target_os = "windows") { "PATH" }
        else if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" }
        else { "LD_LIBRARY_PATH" };

    debug!("Adding to {}: {}", 
        runtime_link_path_var, RUNTIME_LINK_PATHS_STR);

    let old_runtime_link_paths = env::var(runtime_link_path_var)
        .unwrap_or_default();
    let new_runtime_link_paths = format!(
        "{}{}{}",
        RUNTIME_LINK_PATHS_STR,
        common::fs::PATH_LIST_SEPARATOR,
        old_runtime_link_paths
    );
    // Set the modified runtime link paths environment variable.
    env::set_var(runtime_link_path_var, &new_runtime_link_paths);

    // Set other paths as well.
    let shoop_src_root_dir = normalize_path(PathBuf::from(SRC_DIR).join("../../.."));
    env::set_var("SHOOP_OVERRIDE_QML_DIR", shoop_src_root_dir.join("src/qml").to_str().unwrap().to_string());
    env::set_var("SHOOP_OVERRIDE_PY_DIR", shoop_src_root_dir.join("src/python/shoopdaloop").to_str().unwrap().to_string());
    env::set_var("SHOOP_OVERRIDE_LUA_DIR", shoop_src_root_dir.join("src/lua").to_str().unwrap().to_string());
    env::set_var("SHOOP_OVERRIDE_RESOURCE_DIR", shoop_src_root_dir.join("resources").to_str().unwrap().to_string());
    env::set_var("SHOOP_OVERRIDE_SCHEMAS_DIR", shoop_src_root_dir.join("src/session_schemas").to_str().unwrap().to_string());

    // Set PYTHONPATH
    env::set_var("PYTHONPATH", py_env::dev_env_pythonpath().as_str());

    let shoopdaloop_executable = launcher_dir.join("shoopdaloop");

    // Collect all arguments passed to the launcher and pass them to the executable.
    let args: Vec<String> = env::args().skip(1).collect();

    // Prepare to launch the executable in the modified environment with arguments.
    let status = Command::new(shoopdaloop_executable)
        .args(&args) // Pass the arguments here
        .spawn()?
        .wait()?;

    // Exit with the same code as the launched executable.
    exit(status.code().unwrap_or_default());
}
