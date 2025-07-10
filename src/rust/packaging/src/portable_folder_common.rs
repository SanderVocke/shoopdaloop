use crate::dependencies::get_dependency_libs;
use anyhow;
use anyhow::Context;
use common::util::copy_dir_merge;
use copy_dir::copy_dir;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::logging::macros::*;
shoop_log_unit!("packaging");

const MAYBE_QMAKE: Option<&'static str> = option_env!("QMAKE");

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

pub fn populate_portable_folder(
    folder: &Path,
    exe_path: &Path,
    src_path: &Path,
    includelist_path: &Path,
    excludelist_path: &Path,
) -> Result<(), anyhow::Error> {
    let qmake = MAYBE_QMAKE.ok_or(anyhow::anyhow!("QMAKE not set at compile-time"))?;

    let lib_dir = folder.join("lib");
    std::fs::create_dir(&lib_dir).with_context(|| format!("Cannot create dir: {:?}", lib_dir))?;

    let py_lib_dir = lib_dir.join("python");
    std::fs::create_dir(&py_lib_dir)
        .with_context(|| format!("Cannot create dir: {:?}", py_lib_dir))?;

    info!("Bundling executable...");
    let final_exe_filename = if cfg!(target_os = "windows") {
        "shoopdaloop_exe.exe"
    } else {
        "shoopdaloop_exe"
    };
    let final_exe_path = folder.join(final_exe_filename);
    std::fs::copy(exe_path, &final_exe_path)?;

    info!("Bundling development environment Python packages from PYTHONPATH...");
    let python_lib_paths =
        crate::remove_subpaths::remove_subpaths(&py_env::dev_env_pythonpath_entries());
    for path in python_lib_paths {
        if path.ends_with("site-packages") {
            debug!("--> {} -> site-packages", path);
            copy_dir_merge(&path, &py_lib_dir.join("site-packages"))?;
        } else if PathBuf::from(path.clone()).is_dir() {
            debug!("--> {} -> python", path);
            copy_dir_merge(&path, &py_lib_dir)?;
        } else {
            debug!("--> {} -> ignored (not a directory)", path);
        }
    }

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
    for path in backend::runtime_link_dirs() {
        debug!("--> extra search path: {:?}", path);
        common::env::add_lib_search_path(&path);
    }
    // Also include search paths to all of Qt's plugin directories
    let plugin_subdirs = std::fs::read_dir(install_plugins_dir)?;
    for entry in plugin_subdirs {
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
        let dst = lib_dir.clone().join(lib.file_name().unwrap());
        debug!("--> {:?} -> {:?}", &src, &dst);
        std::fs::copy(&src, &dst)?;
    }

    Ok(())
}
