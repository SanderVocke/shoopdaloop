use anyhow;
use anyhow::Context;
use std::path::{PathBuf, Path};
use tempfile::TempDir;
use std::process::Command;
use crate::dependencies::get_dependency_libs;
use crate::fs_helpers::recursive_dir_cpy;

fn populate_folder(
    shoop_built_out_dir : &Path,
    folder : &Path,
    exe_path : &Path,
    dev_exe_path : &Path,
    include_tests : bool,
    release : bool,
) -> Result<(), anyhow::Error> {
    let file_path = PathBuf::from(file!());
    let src_path = std::fs::canonicalize(file_path)?;
    let src_path = src_path.ancestors().nth(5).ok_or(anyhow::anyhow!("cannot find src dir"))?;
    println!("Using source path {src_path:?}");

    println!("Bundling executable...");
    std::fs::copy(exe_path, folder.join("shoopdaloop.exe"))
        .with_context(|| format!("Failed to copy {exe_path:?} to {folder:?}"))?;

    println!("Bundling shoop_lib...");
    let lib_dir = folder.join("shoop_lib");
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
    let dynlib_dir = folder;
    let excludelist_path = src_path.join("distribution/windows/excludelist");
    let includelist_path = src_path.join("distribution/windows/includelist");
    let libs = get_dependency_libs (dev_exe_path, src_path, &excludelist_path, &includelist_path, ".exe", false)?;
    for lib in libs {
        println!("  Bundling {}", lib.to_str().unwrap());
        std::fs::copy(
            lib.clone(),
            dynlib_dir.join(lib.file_name().unwrap())
        )?;
    }

    // println!("Bundling additional assets...");
    // for file in [
    //     "distribution/linux/shoopdaloop.desktop",
    //     "distribution/linux/shoopdaloop.png",
    //     "distribution/linux/AppRun",
    //     "distribution/linux/backend_tests",
    //     "distribution/linux/python_tests",
    //     "distribution/linux/rust_tests",
    // ] {
    //     let from = src_path.join(file);
    //     let to = folder.join(from.file_name().unwrap());
    //     println!("  {:?} -> {:?}", &from, &to);
    //     std::fs::copy(&from, &to)
    //         .with_context(|| format!("Failed to copy {:?} to {:?}", from, to))?;
    // }

    // if !include_tests {
    //     println!("Slimming down folder...");
    //     for file in [
    //         "shoop_lib/test_runner"
    //     ] {
    //         let path = folder.join(file);
    //         println!("  remove {:?}", path);
    //         std::fs::remove_file(&path)?;
    //     }
    // }

    if include_tests {
        println!("Downloading prebuilt cargo-nextest into folder...");
        let nextest_path = folder.join("cargo-nextest.exe");
        let nextest_dir = folder.to_str().unwrap();
        Command::new("sh")
                .current_dir(&src_path)
                .args(&["-c",
                        &format!("curl -LsSf https://get.nexte.st/latest/windows | tar zxf - -C {}", nextest_dir)
                        ])
                .status()?;
        println!("Creating nextest archive...");
        let archive = folder.join("nextest-archive.tar.zst");
        let args = match release {
            true => vec!["nextest", "archive", "--release", "--archive-file", archive.to_str().unwrap()],
            false => vec!["nextest", "archive", "--archive-file", archive.to_str().unwrap()]
        };
        Command::new(&nextest_path)
                .current_dir(&src_path)
                .args(&args[..])
                .status()?;
    }

    println!("Portable folder produced in {}", folder.to_str().unwrap());

    Ok(())
}

pub fn build_portable_folder(
    shoop_built_out_dir : &Path,
    exe_path : &Path,
    dev_exe_path : &Path,
    output_dir : &Path,
    include_tests : bool,
    release : bool,
) -> Result<(), anyhow::Error> {
    println!("Assets directory: {:?}", shoop_built_out_dir);

    if std::fs::exists(output_dir)? {
        return Err(anyhow::anyhow!("Output directory {:?} already exists", output_dir));
    }
    if !std::fs::exists(output_dir.parent().unwrap())? {
        return Err(anyhow::anyhow!("Output directory {:?}: parent doesn't exist", output_dir));
    }
    println!("Creating portable directory...");
    std::fs::create_dir(output_dir)?;

    populate_folder(shoop_built_out_dir,
                            output_dir,
                            exe_path,
                            dev_exe_path,
                            include_tests,
                            release)?;

    println!("Portable folder created @ {output_dir:?}");
    Ok(())
}

