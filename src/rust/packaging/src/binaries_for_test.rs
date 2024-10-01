use anyhow;
use std::path::{PathBuf, Path};
use std::process::Command;

use common::logging::macros::*;
shoop_log_unit!("packaging");

fn populate_folder(
    shoop_built_out_dir : &Path,
    folder : &Path,
    release : bool,
) -> Result<(), anyhow::Error> {
    let file_path = PathBuf::from(file!());
    let src_path = std::fs::canonicalize(file_path)?;
    let src_path = src_path.ancestors().nth(5).ok_or(anyhow::anyhow!("cannot find src dir"))?;
    info!("Using source path {src_path:?}");

    for f in glob::glob(format!("{}/**/test_runner*", shoop_built_out_dir.join("shoop_lib").to_str().unwrap()).as_str())? {
        let f = f?.clone();
        info!("Bundling {f:?}...");
        std::fs::copy(
            &f,
            folder.join(&f.file_name().unwrap())
        )?;
    }

    info!("Downloading prebuilt cargo-nextest into folder...");

    let nextest_path : PathBuf;
    let nextest_dir = folder.to_str().unwrap();
    println!("sh -c 'curl -LsSf https://get.nexte.st/latest/windows-tar | tar zxf - -C {}'", nextest_dir);
    #[cfg(target_os = "windows")]
    {
        nextest_path = folder.join("cargo-nextest.exe");
        Command::new("sh")
                .current_dir(&src_path)
                .args(&["-c",
                        &format!("curl -LsSf https://get.nexte.st/latest/windows-tar | tar zxf - -C {}", nextest_dir)
                        ])
                .status()?;
    }
    #[cfg(target_os = "macos")]
    {
        nextest_path = folder.join("cargo-nextest");
        Command::new("sh")
                .current_dir(&src_path)
                .args(&["-c",
                        &format!("curl -LsSf https://get.nexte.st/latest/mac | tar zxf - -C {}", nextest_dir)
                        ])
                .status()?;
    }
    #[cfg(target_os = "linux")]
    {
        nextest_path = folder.join("cargo-nextest");
        Command::new("sh")
                .current_dir(&src_path)
                .args(&["-c",
                        &format!("curl -LsSf https://get.nexte.st/latest/linux | tar zxf - -C {}", nextest_dir)
                        ])
                .status()?;
    }

    info!("Creating nextest archive...");
    let archive = folder.join("nextest-archive.tar.zst");
    let args = match release {
        true => vec!["nextest", "archive", "--release", "--archive-file", archive.to_str().unwrap()],
        false => vec!["nextest", "archive", "--archive-file", archive.to_str().unwrap()]
    };
    Command::new(&nextest_path)
            .current_dir(&src_path)
            .args(&args[..])
            .status()?;

    info!("Test binaries folder produced in {}", folder.to_str().unwrap());

    Ok(())
}

pub fn build_test_binaries_folder(
    shoop_built_out_dir : &Path,
    output_dir : &Path,
    release : bool,
) -> Result<(), anyhow::Error> {
    if output_dir.exists() {
        return Err(anyhow::anyhow!("Output directory {:?} already exists", output_dir));
    }
    if !output_dir.parent()
        .ok_or(anyhow::anyhow!("Cannot find parent of {output_dir:?}"))?
        .exists() {
        return Err(anyhow::anyhow!("Output directory {:?}: parent doesn't exist", output_dir));
    }
    info!("Creating test binaries directory...");
    std::fs::create_dir(output_dir)?;

    populate_folder(shoop_built_out_dir,
                            output_dir,
                            release)?;

    info!("Test binaries folder created @ {output_dir:?}");
    Ok(())
}

