use anyhow;
use anyhow::Context;
use std::path::{PathBuf, Path};
use std::process::Command;
use std::os::unix::fs::PermissionsExt;
use glob::glob;
use std::collections::{HashSet, HashMap};
use crate::dependencies::get_dependency_libs;
use crate::fs_helpers::recursive_dir_cpy;

fn populate_appdir(
    shoop_built_out_dir : &Path,
    appdir : &Path,
    exe_path : &Path,
    dev_exe_path : &Path,
    release : bool,
) -> Result<(), anyhow::Error> {
    let file_path = PathBuf::from(file!());
    let src_path = std::fs::canonicalize(file_path)?;
    let src_path = src_path.ancestors().nth(5).ok_or(anyhow::anyhow!("cannot find src dir"))?;
    println!("Using source path {src_path:?}");

    println!("Bundling executable...");
    let final_exe_path = appdir.join("shoopdaloop_bin");
    std::fs::copy(exe_path, &final_exe_path)?;

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

    println!("Getting dependencies...");
    let mut all_files_to_analyze : Vec<PathBuf> = Vec::new();
    for f in glob(&format!("{}/**/*.so*", appdir.to_str().unwrap()))
        .with_context(|| "Failed to read glob pattern")? {
            let p = std::fs::canonicalize(f?)?;
            if !p.to_str().unwrap().contains("py/lib/python") &&
               !std::fs::symlink_metadata(&p)?.is_symlink() {
                all_files_to_analyze.push(p);
            }
        }
    all_files_to_analyze.push(final_exe_path.to_owned());
    let all_files_to_analyze_refs : Vec<&Path> = all_files_to_analyze.iter().map(|p| p.as_path()).collect();
    let dynlib_dir = appdir.join("lib");
    let excludelist_path = src_path.join("distribution/linux/excludelist");
    let includelist_path = src_path.join("distribution/linux/includelist");
    let all_lib_dirs : HashSet<String> = all_files_to_analyze_refs.iter().map(|p| String::from(p.parent().unwrap().to_str().unwrap())).collect();
    let all_lib_dirs : Vec<&str> = all_lib_dirs.iter().map(|s| s.as_str()).collect();
    let all_lib_dirs : String = all_lib_dirs.join(":");
    let env : HashMap<&str, &str> = HashMap::from([
        ("LD_LIBRARY_PATH", all_lib_dirs.as_str()),
    ]);
    let libs = get_dependency_libs (&all_files_to_analyze_refs, &env, &excludelist_path, &includelist_path, ".so", false)?;

    println!("Bundling {} dependencies...", libs.len());
    std::fs::create_dir(&dynlib_dir)?;
    for lib in libs {
        println!("  Bundling {}", lib.to_str().unwrap());
        std::fs::copy(
            lib.clone(),
            dynlib_dir.clone().join(lib.file_name().unwrap())
        )?;
    }

    println!("Bundling additional assets...");
    for file in [
        "distribution/linux/shoopdaloop.desktop",
        "distribution/linux/shoopdaloop.png",
        "distribution/linux/shoopdaloop",
        "distribution/linux/AppRun",
        "distribution/linux/backend_tests",
        "distribution/linux/python_tests",
        "distribution/linux/rust_tests",
        "distribution/linux/env.sh",
    ] {
        let from = src_path.join(file);
        let to = appdir.join(from.file_name().unwrap());
        println!("  {:?} -> {:?}", &from, &to);
        std::fs::copy(&from, &to)
            .with_context(|| format!("Failed to copy {:?} to {:?}", from, to))?;
    }

    println!("Slimming down AppDir...");
    for pattern in [
        "shoop_lib/test_runner",
        "shoop_lib/py/lib/python*/test",
        "shoop_lib/py/lib/python*/*/test",
        "**/*.a",
    ] {
        let pattern = format!("{}/{pattern}", appdir.to_str().unwrap());
        for f in glob(&pattern)? {
            let f = f?;
            println!("  remove {f:?}");
            match std::fs::metadata(&f)?.is_dir() {
                true => { std::fs::remove_dir_all(&f)?; }
                false => { std::fs::remove_file(&f)?; }
            }
        }
    }

    println!("AppDir produced in {}", appdir.to_str().unwrap());

    Ok(())
}

pub fn build_appdir(
    shoop_built_out_dir : &Path,
    exe_path : &Path,
    dev_exe_path : &Path,
    output_dir : &Path,
    release : bool,
) -> Result<(), anyhow::Error> {
    println!("Assets directory: {:?}", shoop_built_out_dir);

    if output_dir.exists() {
        return Err(anyhow::anyhow!("Output directory {:?} already exists", output_dir));
    }
    if !output_dir.parent()
       .ok_or(anyhow::anyhow!("Cannot find parent of {output_dir:?}"))?
       .exists() {
        return Err(anyhow::anyhow!("Output directory {:?}: parent doesn't exist", output_dir));
    }
    println!("Creating app directory...");
    std::fs::create_dir(output_dir)?;

    populate_appdir(shoop_built_out_dir,
                    output_dir,
                    exe_path,
                    dev_exe_path,
                    release)?;

    println!("AppDir created @ {output_dir:?}");
    Ok(())
}

