use crate::dependencies::get_dependency_libs;
use crate::fs_helpers::recursive_dir_cpy;
use anyhow::anyhow;
use anyhow::Context;
use common::util::copy_dir_merge;
use copy_dir::copy_dir;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::logging::macros::*;
shoop_log_unit!("packaging");

const MAYBE_QMAKE: Option<&'static str> = option_env!("QMAKE");

/// Qt plugins that get copied in with the rest of the plugin tree but that
/// ShoopDaLoop does not ship.
///
/// Paths are `<plugin subdir>/<basename without prefix or extension>`, so the
/// same entry matches `qsqlpsql.dll`, `libqsqlpsql.dylib` and `libqsqlpsql.so`,
/// plus any accompanying debug-symbol file.
///
/// The dependency walker seeds from every binary in the package, so anything
/// left here has to have its whole dependency closure bundled. The PostgreSQL
/// driver would drag in libpq and, transitively, its own SSL, Kerberos and
/// libintl stack -- for a database an audio looper has no use for.
const UNWANTED_QT_PLUGINS: &[&str] = &["sqldrivers/qsqlpsql"];

fn qmake_command(qmake_path: &str, argstring: &str) -> Command {
    let shell_command = format!("{} {}", qmake_path, argstring);
    return if cfg!(target_os = "windows") {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", format!("{shell_command}").as_str()]);
        cmd
    } else {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", format!("{shell_command}").as_str()]);
        cmd
    };
}

/// Remove the plugins listed in [`UNWANTED_QT_PLUGINS`] from a copied Qt plugin
/// tree, along with any same-named debug-symbol files.
fn prune_unwanted_qt_plugins(install_plugins_dir: &Path) -> Result<(), anyhow::Error> {
    for relative in UNWANTED_QT_PLUGINS {
        let (subdir, stem) = relative
            .split_once('/')
            .ok_or_else(|| anyhow!("Malformed unwanted-plugin entry: {relative}"))?;
        let dir = install_plugins_dir.join(subdir);
        if !dir.is_dir() {
            debug!("--> no {subdir} plugin dir; nothing to prune");
            continue;
        }
        let prefixed = format!("lib{stem}");
        for entry in std::fs::read_dir(&dir).with_context(|| format!("Cannot read {dir:?}"))? {
            let path = entry?.path();
            let Some(file_stem) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
                continue;
            };
            if file_stem == stem || file_stem == prefixed {
                info!("--> Removing unwanted Qt plugin file: {:?}", path);
                std::fs::remove_file(&path).with_context(|| format!("Cannot remove {path:?}"))?;
            }
        }
    }
    Ok(())
}

pub fn populate_portable_folder(
    folder: &Path,
    exe_path: &Path,
    src_path: &Path,
    includelist_path: &Path,
    excludelist_path: &Path,
) -> Result<(), anyhow::Error> {
    let qmake = MAYBE_QMAKE.ok_or(anyhow!("QMAKE not set at compile-time"))?;

    let lib_dir = folder.join("lib");
    std::fs::create_dir(&lib_dir).with_context(|| format!("Cannot create dir: {:?}", lib_dir))?;

    info!("Bundling executable...");
    let final_exe_filename = if cfg!(target_os = "windows") {
        "shoopdaloop_exe.exe"
    } else {
        "shoopdaloop_exe"
    };
    let final_exe_path = folder.join(final_exe_filename);
    std::fs::copy(exe_path, &final_exe_path)?;

    // Copy filesets into our output lib dir
    let to_copy = [
        ("src/lua", "lua"),
        ("src/qml", "qml"),
        ("src/session_schemas", "session_schemas"),
        ("resources", "resources"),
    ];
    info!("Bundling source assets...");
    for (from, to) in to_copy {
        let src = src_path.join(from);
        let dst = folder.join(to);
        debug!("--> {:?} -> {:?}", src, dst);
        copy_dir(&src, &dst)?;
    }

    let qt_install_dir = folder.join("Qt6");
    std::fs::create_dir(&qt_install_dir)
        .with_context(|| format!("Cannot create dir: {:?}", qt_install_dir))?;

    info!("Bundling Qt plugins...");
    let qt_plugins = qmake_command(qmake, "-query QT_INSTALL_PLUGINS")
        .stderr(std::process::Stdio::inherit())
        .output()?;
    let qt_plugins = String::from_utf8(qt_plugins.stdout)?;
    let qt_plugins = PathBuf::from(qt_plugins.trim());
    let install_plugins_dir = folder.join("Qt6/plugins");
    debug!("--> {:?} -> {:?}", qt_plugins, install_plugins_dir);
    copy_dir(qt_plugins, &install_plugins_dir)?;
    prune_unwanted_qt_plugins(&install_plugins_dir)?;

    info!("Bundling Qt QML components...");
    let qt_qml = qmake_command(qmake, "-query QT_INSTALL_QML")
        .stderr(std::process::Stdio::inherit())
        .output()?;
    let qt_qml = String::from_utf8(qt_qml.stdout)?;
    let qt_qml = PathBuf::from(qt_qml.trim());
    let install_qml_dir = folder.join("Qt6/qml");
    debug!("--> {:?} -> {:?}", qt_qml, install_qml_dir);
    copy_dir_merge(qt_qml, &install_qml_dir)?;

    info!("Getting dependencies (this may take some time)...");
    // Build environments expose vcpkg through CMAKE_PREFIX_PATH, but that does
    // not by itself affect the runtime loader used by the dependency scanners.
    // Add its runtime directories explicitly, particularly for @rpath
    // resolution on macOS.
    if let Some(prefixes) = std::env::var_os("CMAKE_PREFIX_PATH") {
        for prefix in std::env::split_paths(&prefixes) {
            let runtime_paths = if cfg!(target_os = "windows") {
                ["debug/bin", "bin", "debug/lib", "lib"]
            } else {
                ["lib", "bin", "debug/lib", "debug/bin"]
            };
            for relative in runtime_paths {
                let path = prefix.join(relative);
                if path.is_dir() {
                    debug!("--> extra vcpkg runtime search path: {:?}", path);
                    common::env::add_lib_search_path(&path);
                }
            }
        }
    }
    // Qt's plugin directories, for the subprocess-based scanners that resolve
    // through the loader's environment.
    //
    // Not needed on Windows: the in-process walker indexes the whole output
    // folder, which already contains these directories, and resolves against an
    // explicit search-directory list rather than PATH.
    #[cfg(not(windows))]
    for entry in std::fs::read_dir(&install_plugins_dir)? {
        let entry = entry?;
        let path = entry.path();
        debug!("--> extra search path: {:?}", path);
        common::env::add_lib_search_path(&path);
    }
    let dependency_libs = get_dependency_libs(
        &final_exe_path,
        folder,
        &excludelist_path,
        &includelist_path,
        false,
    )?;

    info!("Bundling {} dependencies...", dependency_libs.len());
    for lib in dependency_libs {
        let src = lib.clone();
        let dst = lib_dir.clone().join(
            lib.file_name()
                .ok_or(anyhow!("Invalid library path (no filename): {:?}", lib))?,
        );

        if !src.exists() {
            info!("--> Skipping nonexistent file/dir: {src:?}");
        } else if std::fs::metadata(&src)?.is_dir() {
            info!("--> Bundling directory: {:?} -> {:?}", &src, &dst);
            recursive_dir_cpy(&src, &dst)
                .with_context(|| format!("Failed to copy dir {src:?} to {dst:?}"))?;
        } else {
            debug!("--> Bundling file: {:?} -> {:?}", &src, &dst);
            std::fs::copy(&src, &dst)
                .with_context(|| format!("Failed to copy {src:?} to {dst:?}"))?;
        }
    }

    Ok(())
}
