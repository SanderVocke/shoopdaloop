use anyhow;
use anyhow::Context;
use std::path::{PathBuf, Path};
use tempfile::TempDir;
use std::collections::HashSet;
use std::process::Command;
use std::os::unix::fs::PermissionsExt;

fn recursive_dir_cpy (src: &Path, dst: &Path) -> Result<(), anyhow::Error> {
    for entry in std::fs::read_dir(src)
                    .with_context(|| format!("Cannot read dir {src:?}"))? {
        let entry = entry.with_context(|| format!("Invalid entry"))?;
        let path = entry.path();
        let file_name = path.file_name().unwrap();
        if path.is_dir() {
            std::fs::create_dir(dst.join(file_name))
                .with_context(|| format!("Cannot create {:?}", dst.join(file_name)))?;
            recursive_dir_cpy(&path, &dst.join(file_name))?;
        } else {
            std::fs::copy(&path, &dst.join(file_name))
                .with_context(|| format!("Cannot copy {:?} to {:?}", path, dst.join(file_name)))?;
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

    let excludelist = std::fs::read_to_string(excludelist_path)
                            .with_context(|| format!("Cannot read {excludelist_path:?}"))?;
    let includelist = std::fs::read_to_string(includelist_path)
                            .with_context(|| format!("Cannot read {excludelist_path:?}"))?;
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

fn populate_appdir(
    shoop_built_out_dir : &Path,
    appdir : &Path,
    exe_path : &Path,
    dev_exe_path : &Path,
    include_tests : bool,
) -> Result<(), anyhow::Error> {
    let file_path = PathBuf::from(file!());
    let src_path = std::fs::canonicalize(file_path)?;
    let src_path = src_path.ancestors().nth(5).ok_or(anyhow::anyhow!("cannot find src dir"))?;
    println!("Using source path {src_path:?}");

    println!("Bundling executable...");
    std::fs::copy(exe_path, appdir.join("shoopdaloop"))?;

    println!("Bundling shoop_lib...");
    let lib_dir = appdir.join("shoop_lib");
    std::fs::create_dir(&lib_dir)
        .with_context(|| format!("Cannot create dir: {:?}", lib_dir))?;
    recursive_dir_cpy(
        &PathBuf::from(shoop_built_out_dir).join("shoop_lib"),
        &lib_dir
    )?;

    let target_py_env = lib_dir.join("py");
    println!("Bundling Python environment...");
    {
        let from = PathBuf::from(shoop_built_out_dir).join("shoop_pyenv");
        std::fs::create_dir(&target_py_env)?;
        recursive_dir_cpy(
            &from,
            &target_py_env
        )?;
    }

    println!("Bundling dependencies...");
    let dynlib_dir = appdir.join("lib");
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
        "distribution/appimage/backend_tests",
        "distribution/appimage/python_tests",
        "distribution/appimage/rust_tests",
    ] {
        let from = src_path.join(file);
        let to = appdir.join(from.file_name().unwrap());
        println!("  {:?} -> {:?}", &from, &to);
        std::fs::copy(&from, &to)
            .with_context(|| format!("Failed to copy {:?} to {:?}", from, to))?;
    }

    if !include_tests {
        println!("Slimming down AppDir...");
        for file in [
            "shoop_lib/test_runner"
        ] {
            let path = appdir.join(file);
            println!("  remove {:?}", path);
            std::fs::remove_file(&path)?;
        }
    }

    if include_tests {
        println!("Creating nextest archive...");
        let archive = appdir.join("nextest-archive.tar.zst");
        Command::new("cargo")
                .current_dir(&src_path)
                .args(&["nextest", "archive", "--archive-file", archive.to_str().unwrap()])
                .status()?;
        println!("Downloading prebuilt cargo-nextest into appdir...");
        let nextest_path = appdir.join("cargo-nextest");
        let nextest_dir = appdir.to_str().unwrap();
        Command::new("sh")
                .current_dir(&src_path)
                .args(&["-c",
                        &format!("curl -LsSf https://get.nexte.st/latest/linux | tar zxf - -C {}", nextest_dir)
                        ])
                .status()?;
        let mut perms = std::fs::metadata(nextest_path.clone())
            .with_context(|| "Failed to get file metadata")?
            .permissions();
        perms.set_mode(0o755); // Sets read, write, and execute permissions for the owner, and read and execute for group and others
        std::fs::set_permissions(nextest_path, perms).with_context(|| "Failed to set file permissions")?;
    }

    println!("AppDir produced in {}", appdir.to_str().unwrap());

    Ok(())
}

pub fn build_appimage(
    appimagetool : &str,
    shoop_built_out_dir : &Path,
    exe_path : &Path,
    dev_exe_path : &Path,
    output_file : &Path,
    include_tests : bool
) -> Result<(), anyhow::Error> {
    println!("Assets directory: {:?}", shoop_built_out_dir);

    println!("Creating temporary appdir directory...");
    let tmp_dir = TempDir::new()?;
    let appdir : PathBuf;
    {
        appdir = tmp_dir.path().to_owned();
    }

    match populate_appdir(shoop_built_out_dir,
                    &appdir,
                    exe_path,
                    dev_exe_path,
                    include_tests)
    {
        Ok(()) => Ok(()),
        Err(e) => {
            let p = tmp_dir.into_path();
            eprintln!("AppDir creation failed. Persisted temporary directory @ {:?}", p);
            Err(e)
        }
    }?;

    println!("Creating AppImage...");
    Command::new(appimagetool)
        .args(&[appdir.to_str().unwrap(), output_file.to_str().unwrap()])
        .status()?;

    println!("AppImage created @ {output_file:?}");
    Ok(())
}

