use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::collections::HashSet;
use glob::glob;
use anyhow;
use anyhow::Context;
use copy_dir::copy_dir;

// Include generated source for remembering the OUT_DIR at build time.
const SHOOP_BUILD_OUT_DIR : Option<&str> = option_env!("OUT_DIR");

fn recursive_dir_cpy (src: &Path, dst: &Path) -> std::io::Result<()> {
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

fn get_dependency_libs (exe : &Path,
                        src_dir : &Path,
                        excludelist_path : &Path,
                        includelist_path : &Path) -> Result<Vec<PathBuf>, anyhow::Error> {
    let mut rval : Vec<PathBuf> = Vec::new();
    let mut error_msgs : String = String::from("");

    let excludelist = std::fs::read_to_string(excludelist_path).unwrap();
    let includelist = std::fs::read_to_string(includelist_path).unwrap();
    let excludes : HashSet<&str> = excludelist.lines().collect();
    let includes : HashSet<&str> = includelist.lines().collect();
    let mut used_includes : HashSet<String> = HashSet::new();

    let command = "sh";
    let list_deps_script = src_dir.join("scripts/list_dependencies.sh");
    let args = [
        list_deps_script.to_str().unwrap(),
        exe.to_str().unwrap(),
        "-b", "dummy",
        "--quit-after", "1"
    ];
    println!("Running command for determining dependencies: {} {}", command, args.join(" "));
    let list_deps_output = Command::new(command)
                             .args(&args)
                             .output()
                             .with_context(|| "Failed to run list_dependencies")?;
    let command_output = std::str::from_utf8(&list_deps_output.stderr)?;
    let deps_output = std::str::from_utf8(&list_deps_output.stdout)?;
    println!("Command stderr:\n{}", command_output);
    println!("Command stdout:\n{}", deps_output);

    for line in deps_output.lines() {
        if line.trim().is_empty() {
            continue;
        }
        // Extract the full path to the library
        let path = PathBuf::from(line.trim());
        let path_str = path.to_str().ok_or(anyhow::anyhow!("cannot find dependency"))?;
        let path_filename = path.file_name().unwrap().to_str().unwrap();
        if path_filename.contains(".so") {
            let pattern_match = |s : &str, p : &str| regex::Regex::new(
                &s.replace(".", "\\.")
                    .replace("*", ".*")
                    .replace("+", "\\+")
            ).unwrap().is_match(p);
            let in_excludes = excludes.iter().any(|e| pattern_match(e, path_str));
            let in_includes = includes.iter().any(|e| pattern_match(e, path_str));
            if !path.exists() {
                error_msgs.push_str(format!("{}: doesn't exist\n", path_str).as_str());
                continue;
            } else if in_excludes && in_includes {
                error_msgs.push_str(format!("{}: is in includes and excludes\n", path_str).as_str());
                continue;
            } else if in_excludes {
                println!("  Note: skipping excluded dependency {}", &path_str);
                continue;
            } else if !in_includes {
                error_msgs.push_str(format!("{}: is not in include list\n", path_str).as_str());
                continue;
            }

            used_includes.insert(path_filename.to_string());
            rval.push(path);
        } else {
            println!("  Note: skipped ldd line (not a .so): {}", line);
        }
    }

    if !error_msgs.is_empty() {
        return Err(anyhow::anyhow!("Dependency errors:\n{}", error_msgs));
    }

    for include in includes {
        if !used_includes.contains(include) {
            println!("  Note: library {} from include list was not required", include);
        }
    }

    Ok(rval)
}

fn main_impl() -> Result<(), anyhow::Error> {
    let args: Vec<String> = env::args().collect();
    let file_path = PathBuf::from(file!());
    let src_path = file_path.ancestors().nth(6).ok_or(anyhow::anyhow!("cannot find src dir"))?;

    let usage = || {
        println!("Usage: build_appdir path/to/shoopdaloop path/to/shoopdaloop_dev target_dir");
        println!("  the target directory should not exist, but its parent should.");
        std::process::exit(1);
    };

    let bad_usage = |msg : _| {
        println!("Bad usage: {}", msg);
        usage();
    };

    if args.len() != 4 {
        bad_usage(format!("Wrong number of arguments: {} vs. 3", args.len() - 1));
    }

    let exe_path = Path::new(&args[1]);
    let dev_exe_path = Path::new(&args[2]);
    if !exe_path.is_file() {
        bad_usage("First argument is not an executable file".to_string());
    }

    let target_dir = Path::new(&args[3]);
    if target_dir.exists() {
        bad_usage("Target directory already exists".to_string());
    }
    if !target_dir.parent().unwrap().exists() {
        bad_usage("Target directory parent doesn't exist".to_string());
    }

    let out_dir = SHOOP_BUILD_OUT_DIR.ok_or(anyhow::anyhow!("No OUT_DIR set"))?;
    println!("Assets directory: {}", out_dir);

    println!("Creating target directory...");
    std::fs::create_dir(target_dir)?;

    println!("Bundling executable...");
    std::fs::copy(exe_path, target_dir.join("shoopdaloop"))?;

    println!("Bundling shoop_lib...");
    let lib_dir = target_dir.join("shoop_lib");
    std::fs::create_dir(&lib_dir)?;
    recursive_dir_cpy(
        &PathBuf::from(out_dir).join("shoop_lib"),
        &lib_dir
    )?;

    let target_py_env = lib_dir.join("py");
    println!("Bundling Python environment...");
    {
        let from = PathBuf::from(out_dir).join("shoop_pyenv");
        std::fs::create_dir(&target_py_env)?;
        recursive_dir_cpy(
            &from,
            &target_py_env
        )?;
    }

    println!("Bundling dependencies...");
    let dynlib_dir = target_dir.join("lib");
    std::fs::create_dir(&dynlib_dir)?;
    let excludelist_path = src_path.join("distribution/appimage/excludelist");
    let includelist_path = src_path.join("distribution/appimage/includelist");
    let libs = get_dependency_libs (dev_exe_path, src_path, &excludelist_path, &includelist_path)?;
    for lib in libs {
        println!("  Bundling {}", lib.to_str().unwrap());
        std::fs::copy(
            lib.clone(),
            dynlib_dir.clone().join(lib.file_name().unwrap())
        )?;
    }

    println!("Bundling additional assets...");
    for file in [
        "distribution/appimage/shoopdaloop.desktop",
        "distribution/appimage/shoopdaloop.png",
        "distribution/appimage/AppRun",
    ] {
        let from = src_path.join(file);
        let to = target_dir.join(from.file_name().unwrap());
        println!("  {:?} -> {:?}", &from, &to);
        std::fs::copy(&from, &to)
            .with_context(|| format!("Failed to copy {:?} to {:?}", from, to))?;
    }

    println!("Slimming down AppDir...");
    for file in [
        "shoop_lib/test_runner"
    ] {
        let path = target_dir.join(file);
        println!("  remove {:?}", path);
        std::fs::remove_file(&path)?;
    }

    println!("AppDir produced in {}", target_dir.to_str().unwrap());
    println!("You can use a tool such as appimagetool to create an AppImage from this directory.");

    Ok(())
}

fn main() {
    match main_impl() {
        Ok(()) => (),
        Err(error) => {
            eprintln!("AppDir creation failed: {:?}.\n  Backtrace: {:?}", error, error.backtrace());
            std::process::exit(1);
        }
    }
}