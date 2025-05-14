use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, exit};
use std::io;
use common::logging::macros::*;
shoop_log_unit!("Main");

const PATHS_FILE: &str = "shoop.dllpaths"; // The paths file
const EXECUTABLE_FILE: &str = "shoop.executable"; // The executable descriptor

fn main() -> io::Result<()> {
    // Get the launcher's directory.
    let launcher_path = env::current_exe().expect("Failed to get launcher path");
    let launcher_dir = launcher_path
        .parent()
        .expect("Failed to get launcher directory");

    // Load and parse paths from the paths file
    let paths_file_path = launcher_dir.join(PATHS_FILE);
    let paths_content = fs::read_to_string(&paths_file_path)
        .expect("Failed to read paths file");

    let executable_file_path = launcher_dir.join(EXECUTABLE_FILE);
    let executable = fs::read_to_string(&executable_file_path)
        .expect("Failed to read executable file");

    // Collect paths and prepare to add them to PATH.
    let additional_paths: Vec<PathBuf> = paths_content
        .lines()
        .map(|line| launcher_dir.join(line.trim()))
        .collect();

    // Get the existing PATH and append our additional paths.
    let current_path = env::var("PATH").unwrap_or_default();
    debug!("environment PATH: {}", env::var("PATH").unwrap_or_default());
    let new_path = format!(
        "{};{}",
        env::join_paths(additional_paths.iter().map(|p| p.as_path()))
           .expect("Failed to join paths")
           .to_str().expect("Unable to convert new paths"),
        current_path
    );

    // Set the modified PATH environment variable.
    env::set_var("PATH", &new_path);

    debug!("new PATH: {}", env::var("PATH").unwrap_or_default());

    // Collect all arguments passed to the launcher and pass them to the executable.
    let args: Vec<String> = env::args().skip(1).collect();

    // Prepare to launch the executable in the modified environment with arguments.
    let status = Command::new(launcher_dir.join(executable))
        .args(&args) // Pass the arguments here
        .spawn()?
        .wait()?;

    // Exit with the same code as the launched executable.
    exit(status.code().unwrap_or_default());
}
