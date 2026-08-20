use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

const SOURCE_TREE_MARKER: &str = "SHOOP_SRC_TREE";

fn profile_output_directory(out_directory: &Path) -> Option<&Path> {
    let build_directory = out_directory.parent()?.parent()?;
    if build_directory.file_name() != Some(OsStr::new("build")) {
        return None;
    }
    build_directory.parent()
}

fn relative_path(target: &Path, base: &Path) -> Option<PathBuf> {
    let target = target.components().collect::<Vec<_>>();
    let base = base.components().collect::<Vec<_>>();
    let common = target
        .iter()
        .zip(&base)
        .take_while(|(target, base)| target == base)
        .count();
    if common == 0 {
        return None;
    }

    let mut relative = PathBuf::new();
    for component in &base[common..] {
        if !matches!(component, Component::Normal(_)) {
            return None;
        }
        relative.push("..");
    }
    for component in &target[common..] {
        if !matches!(component, Component::Normal(_)) {
            return None;
        }
        relative.push(component.as_os_str());
    }
    if relative.as_os_str().is_empty() {
        relative.push(".");
    }
    Some(relative)
}

fn write_source_tree_marker() -> Result<(), String> {
    let out_directory = PathBuf::from(
        std::env::var_os("OUT_DIR").ok_or_else(|| "OUT_DIR is unavailable".to_owned())?,
    );
    let profile_directory = profile_output_directory(&out_directory)
        .ok_or_else(|| {
            format!(
                "unexpected Cargo OUT_DIR layout: {}",
                out_directory.display()
            )
        })?
        .canonicalize()
        .map_err(|error| format!("could not resolve the profile directory: {error}"))?;
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .map_err(|error| format!("could not resolve the source root: {error}"))?;
    let relative = relative_path(&source_root, &profile_directory).ok_or_else(|| {
        format!(
            "source root {} cannot be expressed relative to {}",
            source_root.display(),
            profile_directory.display()
        )
    })?;
    std::fs::write(
        profile_directory.join(SOURCE_TREE_MARKER),
        format!("{}\n", relative.display()),
    )
    .map_err(|error| format!("could not write {SOURCE_TREE_MARKER}: {error}"))
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        return;
    }
    if let Err(error) = write_source_tree_marker() {
        panic!("could not generate the native source-tree marker: {error}");
    }
}
