use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow;
use anyhow::Context;
use clap::{Parser, Subcommand};

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
        #[arg(short, long, value_name="/path/to/folder", required = true)]
        output_dir: PathBuf,

        #[arg(short, long, action = clap::ArgAction::SetTrue)]
        include_tests: bool,

        #[arg(short, long, action = clap::ArgAction::SetTrue)]
        release: bool,
    },
    BuildAppImage {
        #[arg(short, long, value_name="/path/to/appimagetool", required = true)]
        appimagetool: String,

        #[arg(short, long, value_name="/path/to/AppDir", required = true)]
        appdir: PathBuf,

        #[arg(short, long, value_name="File.AppImage", required = true)]
        output: PathBuf,

        #[arg(short, long, action = clap::ArgAction::SetTrue)]
        strip: bool,
    },
}

pub fn main_impl() -> Result<(), anyhow::Error> {
    let args = Cli::parse();

    let exe = std::env::current_exe()?;
    let exe_dir = exe.parent().ok_or(anyhow::anyhow!("Unable to find exe dir"))?;
    let main_exe = exe_dir.join("shoopdaloop");
    let dev_exe = exe_dir.join("shoopdaloop_dev");
    let print_out_dir_exe = exe_dir.join("print_out_dir");

    let out_dir = Command::new(print_out_dir_exe)
                              .output()
                              .with_context(|| "failed to run parse_out_dir")
                              .and_then(|r| match r.status.success() {
                                true => Ok(std::str::from_utf8(&r.stdout).unwrap().trim().to_string()),
                                false => Err(anyhow::anyhow!("failed to run and parse print_out_dir result"))
                              })?;

    match &args.command {
        Some(Commands::BuildPortableFolder { output_dir, include_tests, release }) => {
            #[cfg(target_os = "linux")]
            {
                use packaging::linux_appdir::build_appdir;
                build_appdir
                           (Path::new(out_dir.as_str()),
                            main_exe.as_path(),
                            dev_exe.as_path(),
                            output_dir.as_path(),
                            *include_tests,
                            *release)
            }
            #[cfg(target_os = "macos")]
            {
                use packaging::macos_appbundle::build_appbundle;
                build_appbundle(
                            Path::new(out_dir.as_str()),
                            main_exe.as_path(),
                            dev_exe.as_path(),
                            output_dir.as_path(),
                            *include_tests,
                            *release)
            }
            #[cfg(target_os = "windows")]
            {
                use packaging::windows_portable_folder::build_portable_folder;
                build_portable_folder(
                            Path::new(out_dir.as_str()),
                            main_exe.as_path(),
                            dev_exe.as_path(),
                            output_dir.as_path(),
                            *include_tests,
                            *release)
            }
        },
        Some(Commands::BuildAppImage { appimagetool, appdir, output, strip }) => {
            #[cfg(target_os = "linux")]
            {
                use packaging::linux_appimage::build_appimage;
                build_appimage(appimagetool, appdir, output, *strip)
            }
            #[cfg(not(target_os = "linux"))]
            {
                let _ = (appimagetool, appdir, output, strip);
                Err(anyhow::anyhow!("AppImage packaging is only supported on Linux systems."))
            }
        },
        _ => Err(anyhow::anyhow!("Did not determine a command to run."))
    }
}

fn main() {
    match main_impl() {
        Ok(()) => (),
        Err(error) => {
            eprintln!("packaging failed: {:?}.\n  Backtrace: {:?}", error, error.backtrace());
            std::process::exit(1);
        }
    }
}