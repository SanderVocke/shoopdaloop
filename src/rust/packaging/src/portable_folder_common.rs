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
        // The platform naming variants of one logical plugin: a `lib` prefix on
        // Unix, and the `d` suffix Qt appends to debug builds on Windows
        // (qsqlpsql.dll in release, qsqlpsqld.dll in debug). Missing the debug
        // variant meant release jobs pruned the plugin and debug jobs did not.
        let variants = [
            stem.to_string(),
            format!("lib{stem}"),
            format!("{stem}d"),
            format!("lib{stem}d"),
        ];
        let mut removed = 0;
        for entry in std::fs::read_dir(&dir).with_context(|| format!("Cannot read {dir:?}"))? {
            let path = entry?.path();
            let Some(file_stem) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
                continue;
            };
            if variants.iter().any(|v| *v == file_stem) {
                info!("--> Removing unwanted Qt plugin file: {:?}", path);
                std::fs::remove_file(&path).with_context(|| format!("Cannot remove {path:?}"))?;
                removed += 1;
            }
        }
        if removed == 0 {
            // Not fatal: the plugin may not be built in this configuration. Worth
            // seeing, though, because the other explanation is that the naming
            // changed and the prune silently stopped working -- which surfaces
            // later as an unexplained unresolved dependency.
            warn!(
                "--> Nothing to prune for {relative} in {:?}. If that plugin is \
                 present under another name, its dependencies will be bundled.",
                dir
            );
        }
    }
    Ok(())
}

/// The file name a library declares for itself, if it declares one.
///
/// On macOS this is the basename of `LC_ID_DYLIB` -- the name dyld treats as the
/// library's identity, and therefore the name the file has to be reachable under.
/// Two files that declare the same install name are two copies of one library
/// however differently they are named on disk.
///
/// `None` for executables, loadable bundles, and platforms where this does not
/// apply; the caller then falls back to the file's own resolved name.
#[cfg(target_os = "macos")]
fn declared_library_name(path: &Path) -> Option<std::ffi::OsString> {
    let info = crate::macho::read_macho_file(path).ok().flatten()?;
    let install_name = info.install_name?;
    let base = install_name.rsplit('/').next()?;
    if base.is_empty() {
        return None;
    }
    Some(std::ffi::OsString::from(base))
}

#[cfg(not(target_os = "macos"))]
fn declared_library_name(_path: &Path) -> Option<std::ffi::OsString> {
    None
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

    // A versioned library is referenced under more than one name: the executable
    // might link `libQt6Core.6.dylib` while a QML plugin links
    // `libQt6Core.6.9.1.dylib`. Copying each referenced name as its own regular
    // file puts two *distinct* files in the bundle, and the loader treats those
    // as two separate libraries. On macOS that surfaces as
    // "Class QMetalLayer is implemented in both ..." from the Objective-C
    // runtime, followed by the Qt platform plugin failing to initialise because
    // it linked against whichever copy the application did not get.
    //
    // So each library is bundled once under one canonical name, with the other
    // referenced names reproduced as symlinks to it.
    //
    // The canonical name comes from the library's own `LC_ID_DYLIB` where it has
    // one, not from resolving symlinks. Resolving symlinks is not enough: vcpkg's
    // macOS Qt installs `libQt6Core.6.dylib` and `libQt6Core.6.9.1.dylib` as two
    // separate regular files, so canonicalizing merges nothing -- while both
    // declare the same install name, which is the identity dyld actually uses.
    let mut aliases: Vec<(PathBuf, std::ffi::OsString)> = Vec::new();

    for lib in dependency_libs {
        let src = lib.clone();
        let requested_name = lib
            .file_name()
            .ok_or(anyhow!("Invalid library path (no filename): {:?}", lib))?
            .to_owned();
        let dst = lib_dir.join(&requested_name);

        if !src.exists() {
            info!("--> Skipping nonexistent file/dir: {src:?}");
            continue;
        }
        if std::fs::metadata(&src)?.is_dir() {
            // Frameworks are directories and are copied whole.
            info!("--> Bundling directory: {:?} -> {:?}", &src, &dst);
            recursive_dir_cpy(&src, &dst)
                .with_context(|| format!("Failed to copy dir {src:?} to {dst:?}"))?;
            continue;
        }

        let real = src
            .canonicalize()
            .with_context(|| format!("Cannot canonicalize {src:?}"))?;
        let canonical_name = declared_library_name(&real).unwrap_or(
            real.file_name()
                .ok_or(anyhow!("Invalid library path (no filename): {:?}", real))?
                .to_owned(),
        );
        let canonical_dst = lib_dir.join(&canonical_name);

        if !canonical_dst.exists() {
            debug!("--> Bundling file: {:?} -> {:?}", &real, &canonical_dst);
            std::fs::copy(&real, &canonical_dst)
                .with_context(|| format!("Failed to copy {real:?} to {canonical_dst:?}"))?;
        }
        if canonical_name != requested_name {
            aliases.push((dst, canonical_name));
        }
    }

    for (link, target) in aliases {
        // `symlink_metadata` rather than `exists`, so a dangling link created by
        // an earlier step is still recognised as present.
        if std::fs::symlink_metadata(&link).is_ok() {
            continue;
        }
        #[cfg(unix)]
        {
            debug!("--> Symlinking {:?} -> {:?}", link, target);
            std::os::unix::fs::symlink(&target, &link)
                .with_context(|| format!("Failed to symlink {link:?} -> {target:?}"))?;
        }
        // Windows has no symlink chains in a vcpkg tree, so this should be
        // unreachable there. Copying keeps it correct if it ever is not, since
        // creating a symlink on Windows needs privileges CI may not have.
        #[cfg(not(unix))]
        {
            let source = lib_dir.join(&target);
            warn!(
                "--> Duplicating {:?} as {:?} (no symlinks here)",
                source, link
            );
            std::fs::copy(&source, &link)
                .with_context(|| format!("Failed to copy {source:?} to {link:?}"))?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every platform's naming of one logical plugin has to be pruned.
    ///
    /// The debug variant is the reason this test exists: Qt appends `d` to plugin
    /// names in Windows debug builds, so release packaging pruned the PostgreSQL
    /// driver while debug packaging kept it and then failed on its dependency.
    #[test]
    fn prunes_every_platform_naming_variant() {
        let root = std::env::temp_dir().join("shoop_prune_variants_test");
        let _ = std::fs::remove_dir_all(&root);
        let sqldrivers = root.join("sqldrivers");
        std::fs::create_dir_all(&sqldrivers).unwrap();

        let pruned = [
            "qsqlpsql.dll",
            "qsqlpsqld.dll",
            "libqsqlpsql.dylib",
            "libqsqlpsql.so",
            // Debug symbols alongside the plugin go too.
            "qsqlpsql.pdb",
        ];
        // Must survive: a different driver, and one whose name merely starts the
        // same way.
        let kept = ["qsqlite.dll", "qsqlpsqlx.dll"];

        for name in pruned.iter().chain(kept.iter()) {
            std::fs::write(sqldrivers.join(name), b"x").unwrap();
        }

        prune_unwanted_qt_plugins(&root).unwrap();

        for name in pruned {
            assert!(
                !sqldrivers.join(name).exists(),
                "{name} should have been pruned"
            );
        }
        for name in kept {
            assert!(sqldrivers.join(name).exists(), "{name} must be kept");
        }

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A missing plugin directory is not an error: not every configuration builds
    /// every plugin.
    #[test]
    fn pruning_an_absent_plugin_dir_is_not_an_error() {
        let root = std::env::temp_dir().join("shoop_prune_absent_test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        prune_unwanted_qt_plugins(&root).expect("an absent plugin dir must not fail");
        let _ = std::fs::remove_dir_all(&root);
    }
}
