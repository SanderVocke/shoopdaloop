use std::process::Command;
use std::env;
use std::path::{Path, PathBuf};

fn recursive_dir_cpy (src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir(dst).unwrap();
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = path.file_name().unwrap();
        if path.is_dir() {
            std::fs::create_dir(dst.join(file_name))?;
            recursive_dir_cpy(&path, &dst.join(file_name))?;
        } else {
            std::fs::copy(&path, &dst.join(file_name))?;
        }
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let usage = || {
        println!("Usage: build_shoop_package shoopdaloop_exe target_dir");
        println!("  the target directory should not exist, but its parent should.");
        std::process::exit(1);
    };
    
    let bad_usage = |msg : _| {
        println!("Bad usage: {}", msg);
        usage();
    };

    if args.len() != 3 {
        bad_usage(format!("Wrong number of arguments: {} vs. {}", args.len(), 3));
    }

    let exe_path = Path::new(&args[1]);
    if !exe_path.is_file() {
        bad_usage("First argument is not an executable file".to_string());
    }

    let target_dir = Path::new(&args[2]);
    if target_dir.exists() {
        bad_usage("Target directory already exists".to_string());
    }
    if !target_dir.parent().unwrap().exists() {
        bad_usage("Target directory parent doesn't exist".to_string());
    }

    println!("Finding assets...");
    let output = Command::new(&exe_path)
        .args(
            &["--shoop-build-print-out-dir"]
        )
        .output()
        .expect("Failed to determine assets directory");
    if !output.status.success() {
        println!("Failed to determine assets directory");
        std::process::exit(1);
    }
    let build_out_dir = String::from_utf8(output.stdout).unwrap();
    println!("Found assets directory: {}", build_out_dir);

    println!("Creating target directory...");
    std::fs::create_dir(target_dir).unwrap();

    println!("Copying executable...");
    std::fs::copy(exe_path, target_dir.join(exe_path.file_name().unwrap())).unwrap();

    println!("Copying assets...");
    std::fs::create_dir(target_dir.join("shoop_lib")).unwrap();
    recursive_dir_cpy(&PathBuf::from(build_out_dir).join("shoopdaloop_venv"), &target_dir.join("shoop_lib").join("py")).unwrap();

    println!("...Done.");
}