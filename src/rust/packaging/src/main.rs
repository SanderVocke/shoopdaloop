#[cfg(not(feature = "prebuild"))]
use anyhow::anyhow;
#[cfg(not(feature = "prebuild"))]
use clap::{Parser, Subcommand};
#[cfg(not(feature = "prebuild"))]
use common;
#[cfg(not(feature = "prebuild"))]
use packaging::binaries_for_test::build_test_binaries_folder;
#[cfg(not(feature = "prebuild"))]
use std::path::PathBuf;

#[cfg(not(feature = "prebuild"))]
use common::logging::macros::*;
#[cfg(not(feature = "prebuild"))]
shoop_log_unit!("packaging");

#[cfg(not(feature = "prebuild"))]
#[derive(Parser)]
#[command(name = "package")]
#[command(about = "in-tree packaging tool for ShoopDaLoop")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[cfg(not(feature = "prebuild"))]
#[derive(Subcommand)]
enum Commands {
    BuildPortableFolder {
        #[arg(short, long, value_name = "/path/to/folder", required = true)]
        output_dir: PathBuf,

        #[arg(long, action = clap::ArgAction::SetTrue)]
        replace: bool,
    },
    BuildTestBinaries {
        #[arg(short, long, value_name = "/path/to/folder", required = true)]
        output_dir: PathBuf,

        #[arg(long)]
        cargo_profile: String,

        #[arg(long, action = clap::ArgAction::SetTrue)]
        replace: bool,
    },
    /// Report the dependency closure of an existing package folder.
    ///
    /// Needs nothing from the build environment -- no qmake, no vcpkg, no freshly
    /// built executable -- so it can be pointed at a downloaded release
    /// artifact. Used to bootstrap and audit the include/exclude lists.
    ///
    /// Note that in a real build `lib/` is still empty when the scan runs, so
    /// libraries reported here as "in folder" from a finished package would be
    /// reported as "to copy" during packaging. Both are correct.
    ScanDependencies {
        #[arg(short, long, value_name = "/path/to/folder", required = true)]
        folder: PathBuf,

        /// Defaults to distribution/<os>/includelist
        #[arg(long, value_name = "FILE")]
        includelist: Option<PathBuf>,

        /// Defaults to distribution/<os>/excludelist
        #[arg(long, value_name = "FILE")]
        excludelist: Option<PathBuf>,

        /// Extra search directory, highest priority. Repeatable.
        #[arg(long = "search-dir", value_name = "DIR")]
        search_dirs: Vec<PathBuf>,

        /// Also search CMAKE_PREFIX_PATH x {debug/bin,bin,debug/lib,lib}
        #[arg(long, action = clap::ArgAction::SetTrue)]
        use_cmake_prefix_path: bool,

        /// Omit %SystemRoot%\System32 and %SystemRoot%
        #[arg(long, action = clap::ArgAction::SetTrue)]
        no_system_dirs: bool,

        /// Collect problems into the report and exit 0 instead of failing.
        #[arg(long, action = clap::ArgAction::SetTrue)]
        report_only: bool,

        /// Emit paste-ready include/exclude list entries.
        #[arg(long, action = clap::ArgAction::SetTrue)]
        print_list_candidates: bool,

        /// Seed only the executable, reproducing the old single-root traversal.
        #[arg(long, action = clap::ArgAction::SetTrue)]
        legacy_root_only: bool,

        /// Stop descending at this depth (the old scanner used 4).
        #[arg(long, value_name = "N")]
        max_depth: Option<usize>,
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
}

#[cfg(not(feature = "prebuild"))]
pub fn main_impl() -> Result<(), anyhow::Error> {
    common::init()?;

    let args = Cli::parse();

    let exe = std::env::current_exe()?;
    let exe_dir = exe.parent().ok_or(anyhow!("Unable to find exe dir"))?;
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
        Some(Commands::ScanDependencies {
            folder,
            includelist,
            excludelist,
            search_dirs,
            use_cmake_prefix_path,
            no_system_dirs,
            report_only,
            print_list_candidates,
            legacy_root_only,
            max_depth,
        }) => {
            let options = packaging::scan::ScanOptions {
                folder: folder.clone(),
                includelist: includelist.clone(),
                excludelist: excludelist.clone(),
                extra_search_dirs: search_dirs.clone(),
                use_cmake_prefix_path: *use_cmake_prefix_path,
                no_system_dirs: *no_system_dirs,
                report_only: *report_only,
                print_list_candidates: *print_list_candidates,
                legacy_root_only: *legacy_root_only,
                max_depth: *max_depth,
            };
            let report = packaging::scan::run_scan(&options)?;
            if !*report_only && report.problem_count() > 0 {
                return Err(anyhow!(
                    "{} dependencies could not be classified or resolved",
                    report.problem_count()
                ));
            }
            Ok(())
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
                Err(anyhow!(
                    "AppImage packaging is only supported on Linux systems."
                ))
            }
        }
        _ => Err(anyhow!("Did not determine a command to run.")),
    }
}

#[cfg(not(feature = "prebuild"))]
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

#[cfg(feature = "prebuild")]
fn main() {}
