//! End-to-end test of `get_dependency_libs` on Windows, against real binaries.
//!
//! The unit tests cover the walker and the resolvers in isolation. This covers
//! the remaining seam: the flat tree the walker produces being consumed by
//! `collect_deps` and turned into a set of files to copy.
//!
//! Gated on `SHOOP_TEST_PACKAGE_DIR`, since it needs real PE files and a
//! machine-specific path cannot be committed:
//!
//! ```text
//! SHOOP_TEST_PACKAGE_DIR=<extracted portable folder> cargo test -p packaging
//! ```
//!
//! This is a separate integration-test binary because it sets `CMAKE_PREFIX_PATH`
//! for the whole process, which is only safe when nothing else runs alongside it.

#![cfg(windows)]

use std::path::{Path, PathBuf};

fn package_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("SHOOP_TEST_PACKAGE_DIR")?);
    if !dir.is_dir() {
        eprintln!("SHOOP_TEST_PACKAGE_DIR is not a directory: {dir:?}");
        return None;
    }
    Some(dir)
}

fn distribution_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../distribution/windows")
}

fn copy_dir_flat(from: &Path, to: &Path) -> std::io::Result<usize> {
    std::fs::create_dir_all(to)?;
    let mut count = 0;
    for entry in std::fs::read_dir(from)? {
        let path = entry?.path();
        if path.is_file() {
            std::fs::copy(&path, to.join(path.file_name().unwrap()))?;
            count += 1;
        }
    }
    Ok(count)
}

/// Build a package folder that mirrors the real layout at the moment the scan
/// runs -- executable in place, `lib/` still empty -- with a stand-in vcpkg
/// prefix supplying the libraries, then check what gets selected for bundling.
#[test]
fn resolves_and_selects_the_executables_dependencies() {
    let Some(package) = package_dir() else {
        eprintln!("skipping: SHOOP_TEST_PACKAGE_DIR not set");
        return;
    };
    let real_exe = package.join("shoopdaloop_exe.exe");
    let real_lib = package.join("lib");
    if !real_exe.is_file() || !real_lib.is_dir() {
        eprintln!("skipping: {package:?} is not a portable folder layout");
        return;
    }

    let scratch = std::env::temp_dir().join("shoop_dep_closure_test");
    let _ = std::fs::remove_dir_all(&scratch);
    let folder = scratch.join("package");
    let prefix = scratch.join("vcpkg");
    std::fs::create_dir_all(&folder).unwrap();

    // The libraries the walker is expected to find, standing in for vcpkg's bin.
    let staged = copy_dir_flat(&real_lib, &prefix.join("bin")).unwrap();
    assert!(staged > 0, "no libraries to resolve against");

    // The executable, and an empty lib/ exactly as `populate_portable_folder`
    // leaves it before the scan.
    std::fs::copy(&real_exe, folder.join("shoopdaloop_exe.exe")).unwrap();
    std::fs::create_dir_all(folder.join("lib")).unwrap();

    std::env::set_var("CMAKE_PREFIX_PATH", &prefix);

    let distribution = distribution_dir();
    let selected = packaging::dependencies::get_dependency_libs(
        &folder.join("shoopdaloop_exe.exe"),
        &folder,
        &distribution.join("excludelist"),
        &distribution.join("includelist"),
        false,
    )
    .expect("the executable's dependency closure must resolve cleanly");

    let names: Vec<String> = selected
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_lowercase())
        .collect();

    // The executable's own statically imported Qt modules, plus their transitive
    // closure, all sourced from the stand-in prefix.
    for expected in [
        "qt6core.dll",
        "qt6gui.dll",
        "qt6qml.dll",
        "qt6quick.dll",
        "qt6widgets.dll",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "{expected} should have been selected; got {names:?}"
        );
    }

    // Everything selected must actually exist -- a path that survives selection
    // but has no file is silently skipped by the copy loop, which is the class of
    // silent hole this whole change removes.
    for path in &selected {
        assert!(path.is_file(), "selected a nonexistent path: {path:?}");
        assert!(
            path.starts_with(&prefix),
            "selected something from outside the stand-in prefix: {path:?}"
        );
    }

    // System libraries must not be bundled.
    for forbidden in [
        "kernel32.dll",
        "user32.dll",
        "advapi32.dll",
        "gdi32.dll",
        "ole32.dll",
        "bcryptprimitives.dll",
    ] {
        assert!(
            !names.iter().any(|n| n == forbidden),
            "{forbidden} is provided by Windows and must not be bundled; got {names:?}"
        );
    }

    let _ = std::fs::remove_dir_all(&scratch);
}

/// A dependency that matches the includelist but cannot be found anywhere has to
/// fail the build, naming the library and what needs it.
///
/// Before this change the same situation produced no error at all: the missing
/// library was simply absent from the package, and the failure surfaced only when
/// a user ran the app.
#[test]
fn an_unresolvable_qt_dependency_fails_with_a_useful_message() {
    let Some(package) = package_dir() else {
        eprintln!("skipping: SHOOP_TEST_PACKAGE_DIR not set");
        return;
    };
    let plugin = package
        .join("Qt6/qml/QtQuick/Controls")
        .join("qtquickcontrols2plugin.dll");
    let real_exe = package.join("shoopdaloop_exe.exe");
    if !plugin.is_file() || !real_exe.is_file() {
        eprintln!("skipping: {package:?} does not contain the expected plugin");
        return;
    }

    let scratch = std::env::temp_dir().join("shoop_dep_closure_missing_test");
    let _ = std::fs::remove_dir_all(&scratch);
    let folder = scratch.join("package");
    let plugin_dir = folder.join("Qt6/qml/QtQuick/Controls");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::create_dir_all(folder.join("lib")).unwrap();
    std::fs::copy(&real_exe, folder.join("shoopdaloop_exe.exe")).unwrap();
    // The plugin, with nothing to satisfy its Qt6QuickControls2 import.
    std::fs::copy(&plugin, plugin_dir.join("qtquickcontrols2plugin.dll")).unwrap();

    std::env::set_var("CMAKE_PREFIX_PATH", "");

    let distribution = distribution_dir();
    let error = packaging::dependencies::get_dependency_libs(
        &folder.join("shoopdaloop_exe.exe"),
        &folder,
        &distribution.join("excludelist"),
        &distribution.join("includelist"),
        false,
    )
    .expect_err("a missing includelisted library must fail the build");

    let message = format!("{error:#}");
    assert!(
        message.to_lowercase().contains("qt6quickcontrols2.dll"),
        "the error must name the missing library: {message}"
    );
    assert!(
        message
            .to_lowercase()
            .contains("qtquickcontrols2plugin.dll"),
        "the error must name what needs it: {message}"
    );

    let _ = std::fs::remove_dir_all(&scratch);
}
