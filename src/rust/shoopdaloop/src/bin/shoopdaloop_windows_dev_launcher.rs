use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, exit};
use std::io;

use backend;

const EXECUTABLE_NAME: &str = "shoopdaloop_dev.exe"; // The executable to launch

fn main() -> io::Result<()> {
    // Get the launcher's directory.
    let launcher_path = env::current_exe().expect("Failed to get launcher path");
    let launcher_dir = launcher_path
        .parent()
        .expect("Failed to get launcher directory");

    let dll_pathbuf = backend::backend_build_dir();
    let dll_path = dll_pathbuf.to_string_lossy();

    // Get the existing PATH and append our additional path.
    let current_path = env::var("PATH").unwrap_or_default();
    // println!("PATH: {}", env::var("PATH").unwrap_or_default());
    let new_path = format!(
        "{};{}",
        dll_path,
        current_path
    );

    // Set the modified PATH environment variable.
    env::set_var("PATH", &new_path);

    println!("launcher modified PATH: {}", env::var("PATH").unwrap_or_default());

    // Collect all arguments passed to the launcher and pass them to the executable.
    let args: Vec<String> = env::args().skip(1).collect();

    // Prepare to launch the executable in the modified environment with arguments.
    let status = Command::new(launcher_dir.join(EXECUTABLE_NAME))
        .args(&args) // Pass the arguments here
        .spawn()?
        .wait()?;

    // Exit with the same code as the launched executable.
    exit(status.code().unwrap_or_default());
}
