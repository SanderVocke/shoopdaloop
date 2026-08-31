use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const SOURCE_TREE_MARKER: &str = "SHOOP_SRC_TREE";

fn command_output(program: &str, arguments: &[&str], directory: &Path) -> Option<String> {
    let output = Command::new(program)
        .args(arguments)
        .current_dir(directory)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn git_path(root: &Path, path: &str) -> Option<PathBuf> {
    command_output(
        "git",
        &["rev-parse", "--path-format=absolute", "--git-path", path],
        root,
    )
    .map(PathBuf::from)
}

fn emit_git_rerun_paths(root: &Path) {
    if let Some(head) = git_path(root, "HEAD") {
        println!("cargo:rerun-if-changed={}", head.display());
    }
    if let Some(reference) = command_output("git", &["symbolic-ref", "HEAD"], root)
        .and_then(|reference| git_path(root, &reference))
    {
        println!("cargo:rerun-if-changed={}", reference.display());
    }
    if let Some(packed_refs) = git_path(root, "packed-refs") {
        println!("cargo:rerun-if-changed={}", packed_refs.display());
    }
}

fn emit_build_identity() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let release_version = std::env::var("SHOOP_RELEASE_VERSION")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let revision = command_output("git", &["rev-parse", "--short=8", "HEAD"], &root)
        .unwrap_or_else(|| "unknown".to_owned());
    let branch = std::env::var("GITHUB_HEAD_REF")
        .or_else(|_| std::env::var("GITHUB_REF_NAME"))
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| command_output("git", &["branch", "--show-current"], &root))
        .unwrap_or_else(|| "unknown".to_owned());
    let build_date = std::env::var("SHOOP_BUILD_DATE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| command_output("date", &["-u", "+%Y-%m-%dT%H:%M:%SZ"], &root))
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| format!("Unix timestamp {}", duration.as_secs()))
                .unwrap_or_else(|_| "unknown".to_owned())
        });
    let kind = if release_version.is_some() {
        "release"
    } else {
        "development"
    };
    let version = release_version.unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned());

    println!("cargo:rustc-env=SHOOP_BUILD_KIND={kind}");
    println!("cargo:rustc-env=SHOOP_BUILD_VERSION={version}");
    println!("cargo:rustc-env=SHOOP_BUILD_REVISION={revision}");
    println!("cargo:rustc-env=SHOOP_BUILD_BRANCH={branch}");
    println!("cargo:rustc-env=SHOOP_BUILD_DATE={build_date}");
    println!("cargo:rerun-if-env-changed=SHOOP_RELEASE_VERSION");
    println!("cargo:rerun-if-env-changed=SHOOP_BUILD_DATE");
    emit_git_rerun_paths(&root);
}

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
    emit_build_identity();
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        return;
    }
    if let Err(error) = write_source_tree_marker() {
        panic!("could not generate the native source-tree marker: {error}");
    }
}
