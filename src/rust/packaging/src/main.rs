use anyhow;
use clap::{Parser, Subcommand};
use common;
use packaging::binaries_for_test::build_test_binaries_folder;
use std::path::PathBuf;

use common::logging::macros::*;
shoop_log_unit!("packaging");

#[derive(Parser)]
#[command(name = "package")]
#[command(about = "in-tree packaging tool for ShoopDaLoop")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    BuildPortableFolder {
        #[arg(short, long, value_name = "/path/to/folder", required = true)]
        output_dir: PathBuf,

        #[arg(long, action = clap::ArgAction::SetTrue)]
        replace: bool,
    },
    BuildAppImage {
        #[arg(
            short = 't',
            long,
            value_name = "/path/to/appimagetool",
            required = true
        )]
        appimagetool: String,

        #[arg(short, long, value_name = "/path/to/AppDir", required = true)]
        appdir: PathBuf,

        #[arg(short, long, value_name = "File.AppImage", required = true)]
        output: PathBuf,

        #[arg(long, action = clap::ArgAction::SetTrue)]
        replace: bool,
    },
    BuildTestBinaries {
        #[arg(short, long, value_name = "/path/to/folder", required = true)]
        output_dir: PathBuf,

        #[arg(short, long)]
        cargo_profile: String,

        #[arg(long, action = clap::ArgAction::SetTrue)]
        replace: bool,
    },
}

pub fn main_impl() -> Result<(), anyhow::Error> {
    common::init()?;

    let args = Cli::parse();

    let exe = std::env::current_exe()?;
    let exe_dir = exe
        .parent()
        .ok_or(anyhow::anyhow!("Unable to find exe dir"))?;
    let main_exe: PathBuf;
    #[cfg(target_os = "windows")]
    {
        main_exe = exe_dir.join("shoopdaloop.exe");
    }
    #[cfg(not(target_os = "windows"))]
    {
        main_exe = exe_dir.join("shoopdaloop");
    }

    match &args.command {
        Some(Commands::BuildPortableFolder {
            output_dir,
            replace,
        }) => {
            #[cfg(target_os = "linux")]
            {
                if *replace && std::fs::exists(output_dir)? {
                    info!("Removing existing directory: {}", output_dir.display());
                    std::fs::remove_dir_all(output_dir)?;
                }
                packaging::linux_appdir::build_appdir(main_exe.as_path(), output_dir.as_path())
            }
            #[cfg(target_os = "macos")]
            {
                if *replace && std::fs::exists(output_dir)? {
                    info!("Removing existing directory: {}", output_dir.display());
                    std::fs::remove_dir_all(output_dir)?;
                }
                packaging::macos_appbundle::build_appbundle(
                    main_exe.as_path(),
                    output_dir.as_path(),
                )
            }
            #[cfg(target_os = "windows")]
            {
                if *replace && std::fs::exists(output_dir)? {
                    info!("Removing existing directory: {}", output_dir.display());
                    std::fs::remove_dir_all(output_dir)?;
                }
                packaging::windows_portable_folder::build_portable_folder(
                    main_exe.as_path(),
                    output_dir.as_path(),
                )
            }
        }
        Some(Commands::BuildTestBinaries {
            output_dir,
            cargo_profile,
            replace,
        }) => {
            if *replace && std::fs::exists(output_dir)? {
                info!("Removing existing directory: {}", output_dir.display());
                std::fs::remove_dir_all(output_dir)?;
            }
            build_test_binaries_folder(output_dir.as_path(), cargo_profile.as_str())
        }
        Some(Commands::BuildAppImage {
            appimagetool,
            appdir,
            output,
            replace,
        }) => {
            #[cfg(target_os = "linux")]
            {
                if *replace && std::fs::exists(output)? {
                    info!("Removing existing file: {}", output.display());
                    std::fs::remove_file(output)?;
                }
                packaging::linux_appimage::build_appimage(appimagetool, appdir, output)
            }
            #[cfg(not(target_os = "linux"))]
            {
                let _ = (appimagetool, appdir, output, replace);
                Err(anyhow::anyhow!(
                    "AppImage packaging is only supported on Linux systems."
                ))
            }
        }
        _ => Err(anyhow::anyhow!("Did not determine a command to run.")),
    }
}

fn main() {
    match main_impl() {
        Ok(()) => (),
        Err(error) => {
            error!(
                "packaging failed: {:?}.\n  Backtrace: {:?}",
                error,
                error.backtrace()
            );
            std::process::exit(1);
        }
    }
}
