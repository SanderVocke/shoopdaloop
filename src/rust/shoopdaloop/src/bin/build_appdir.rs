use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::collections::HashSet;
use glob::glob;
use anyhow;

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

fn get_dependency_libs (exec : &Path,
                        excludelist_path : &Path,
                        includelist_path : &Path) -> Result<Vec<PathBuf>, anyhow::Error> {
    let mut rval : Vec<PathBuf> = Vec::new();
    let mut error_msgs : String = String::from("");

    let excludelist = std::fs::read_to_string(excludelist_path).unwrap();
    let includelist = std::fs::read_to_string(includelist_path).unwrap();
    let excludes : HashSet<&str> = excludelist.lines().collect();
    let includes : HashSet<&str> = includelist.lines().collect();
    let mut used_includes : HashSet<String> = HashSet::new();

    let ldd_output = Command::new("ldd")
       .args(&[exec.to_str().unwrap()])
       .output()?;
    let ldd_output = std::str::from_utf8(&ldd_output.stdout)?;

    for line in ldd_output.lines() {
        // Skip status lines and specific libraries
        if line.contains("no version information available")
            || line.contains("linux-vdso")
            || line.contains("ld-linux")
            || line.contains("not a dynamic executable")
        {
            println!("  Note: skipped ldd line (filtered): {}", line);
            continue;
        }

        // Extract the full path to the library
        if let Some(parts) = line.split_once("=>") {
            let mut path_part = parts.1.trim();
            if let Some(parts) = path_part.split_once(" ") {
                path_part = parts.0.trim();
            }
            let path = PathBuf::from(path_part);
            let path_str = path.to_str().ok_or(anyhow::anyhow!("cannot find dependency"))?;
            let path_filename = path.file_name().unwrap().to_str().unwrap();
            if path_filename.contains(".so") {
                let pattern_match = |s : &str, p : &str| regex::Regex::new(
                    &s.replace(".", "\\.")
                      .replace("*", ".*")
                      .replace("+", "\\+")
                ).unwrap().is_match(p);
                let in_excludes = excludes.iter().any(|e| pattern_match(e, path_filename));
                let in_includes = includes.iter().any(|e| pattern_match(e, path_filename));
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
                    error_msgs.push_str(format!("{}: is not in include list\n", path_filename).as_str());
                    continue;
                }

                used_includes.insert(path_filename.to_string());
                rval.push(path);
            } else {
                println!("  Note: skipped ldd line (no =>): {}", line);
            }
        } else {
            println!("  Note: skipped ldd line (no =>): {}", line);
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
        println!("Usage: build_appdir shoopdaloop_executable target_dir");
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

    let out_dir = SHOOP_BUILD_OUT_DIR.ok_or(anyhow::anyhow!("No OUT_DIR set"))?;
    println!("Assets directory: {}", out_dir);

    println!("Creating target directory...");
    std::fs::create_dir(target_dir)?;

    println!("Bundling executable...");
    std::fs::copy(exe_path, target_dir.join("AppRun"))?;

    println!("Bundling shoop_lib...");
    let lib_dir = target_dir.join("shoop_lib");
    std::fs::create_dir(&lib_dir)?;
    recursive_dir_cpy(
        &PathBuf::from(out_dir).join("shoop_lib"),
        &lib_dir
    )?;

    println!("Bundling Python environment...");
    {
        let from_base = PathBuf::from(out_dir).join("shoop_pyenv");
        let from : PathBuf;
        {
            let pattern = format!("{}/**/site-packages", from_base.to_str().unwrap());
            let mut sp_glob = glob(&pattern)?;
            from = sp_glob.next()
                    .expect(format!("No site-packages dir found @ {}", pattern).as_str())?;
        }
        let to = lib_dir.join("py");
        std::fs::create_dir(&to)?;
        recursive_dir_cpy(
            &from,
            &to
        )?;
    }

    println!("Bundling dependencies...");
    let excludelist_path = src_path.join("distribution/appimage/excludelist");
    let includelist_path = src_path.join("distribution/appimage/includelist");
    let libs = get_dependency_libs (exe_path, &excludelist_path, &includelist_path)?;
    let deps_dir = target_dir.join("external_lib");
    std::fs::create_dir(&deps_dir)?;
    for lib in libs {
        println!("  Bundling {}", lib.to_str().unwrap());
        std::fs::copy(
            lib.clone(),
            deps_dir.clone().join(lib.file_name().unwrap())
        )?;
    }

    println!("Bundling desktop and icon files...");
    std::fs::copy(
        src_path.join("distribution").join("appimage").join("shoopdaloop.desktop"),
        target_dir.join("shoopdaloop.desktop")
    )?;
    std::fs::copy(
        src_path.join("distribution").join("appimage").join("shoopdaloop.png"),
        target_dir.join("shoopdaloop.png")
    )?;

    println!("AppDir produced in {}", target_dir.to_str().unwrap());
    println!("You can use a tool such as appimagetool to create an AppImage from this directory.");

    Ok(())
}

fn main() {
    match main_impl() {
        Ok(()) => (),
        Err(error) => {
            println!("AppDir creation failed: {:?}", error);
            std::process::exit(1);
        }
    }
}