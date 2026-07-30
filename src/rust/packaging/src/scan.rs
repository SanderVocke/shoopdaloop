//! Standalone dependency scanning, exposed as `package scan-dependencies`.
//!
//! Runs the same walker the packaging flow uses, against an existing package
//! folder, with no other build inputs: no `qmake`, no vcpkg, no freshly built
//! executable. That makes it possible to inspect and bootstrap the
//! include/exclude lists against a downloaded release artifact, and it is how
//! the walker is validated.

use anyhow::anyhow;
use std::path::PathBuf;

use crate::deps_walker::ScanReport;

// Linux keeps the lddtree-based scanner, so it uses none of the walker plumbing.
#[cfg(any(windows, target_os = "macos"))]
use crate::deps_walker::{FolderIndex, ResolveEnv};
#[cfg(any(windows, target_os = "macos"))]
use std::path::Path;

#[cfg(windows)]
use crate::deps_walker::{build_dependency_tree, Missing, ScanRequest};
#[cfg(windows)]
use crate::list_matcher::ListMatcher;
#[cfg(windows)]
use crate::pe::{machine_of, SearchDirKind, WindowsScanner};
#[cfg(windows)]
use std::collections::BTreeMap;

use common::logging::macros::*;
shoop_log_unit!("packaging");

/// Environment variable holding extra search directories, separated by the
/// platform path separator. An escape hatch so a build-environment-specific
/// resolution problem can be fixed without a code change.
#[cfg(windows)]
pub const EXTRA_SEARCH_DIRS_VAR: &str = "SHOOP_EXTRA_DEP_SEARCH_DIRS";

pub struct ScanOptions {
    pub folder: PathBuf,
    pub includelist: Option<PathBuf>,
    pub excludelist: Option<PathBuf>,
    pub extra_search_dirs: Vec<PathBuf>,
    pub use_cmake_prefix_path: bool,
    pub no_system_dirs: bool,
    pub report_only: bool,
    pub print_list_candidates: bool,
    /// Seed only the main executable, reproducing the old single-root traversal.
    pub legacy_root_only: bool,
    pub max_depth: Option<usize>,
}

/// Search directories in priority order.
///
/// Deliberately excludes `PATH`. The previous scanner resolved through whatever
/// `PATH` happened to contain (the packaging code prepends to it via
/// `common::env::add_lib_search_path`), which made the bundled set depend on the
/// build machine.
#[cfg(windows)]
pub fn windows_search_dirs(
    extra: &[PathBuf],
    use_cmake_prefix_path: bool,
    no_system_dirs: bool,
) -> Vec<(PathBuf, SearchDirKind)> {
    let mut dirs: Vec<(PathBuf, SearchDirKind)> = Vec::new();

    if use_cmake_prefix_path {
        if let Some(prefixes) = std::env::var_os("CMAKE_PREFIX_PATH") {
            for prefix in std::env::split_paths(&prefixes) {
                // Release before debug, deliberately.
                //
                // rustc on MSVC always links the release CRT -- there is no
                // `/MDd` equivalent -- so a debug-built C++ dependency can never
                // match the Rust side. Non-Qt vcpkg libraries have the same file
                // name in `bin` and `debug/bin` (zlib1.dll, harfbuzz.dll, ...), so
                // search order alone decides which flavour gets bundled.
                //
                // This list is in priority order. The previous code passed the
                // same names to `add_lib_search_path`, which *prepends*, so its
                // effective order was the reverse -- release first. Reading the
                // array as priority order silently inverted that and put 28
                // debug-built libraries plus the debug CRT into the release
                // package.
                for relative in ["bin", "debug/bin", "lib", "debug/lib"] {
                    let path = prefix.join(relative);
                    if path.is_dir() {
                        dirs.push((path, SearchDirKind::Vcpkg));
                    }
                }
            }
        }
    }

    for dir in extra {
        if dir.is_dir() {
            dirs.push((dir.clone(), SearchDirKind::Extra));
        }
    }
    if let Some(from_env) = std::env::var_os(EXTRA_SEARCH_DIRS_VAR) {
        for dir in std::env::split_paths(&from_env) {
            if dir.is_dir() {
                dirs.push((dir, SearchDirKind::Extra));
            }
        }
    }

    if !no_system_dirs {
        // `%SystemRoot%\System32` rather than a `GetSystemDirectoryW` call, to
        // avoid pulling a Windows API crate into the packaging tool. This is
        // x64-only packaging, so there is no SysWOW64 redirection to worry
        // about. Six redistributables (the MSVC runtime and dbghelp) are
        // deliberately bundled from here, which is why this tier exists at all
        // -- and why it comes last, and logs every hit.
        if let Some(system_root) = std::env::var_os("SystemRoot") {
            let system_root = PathBuf::from(system_root);
            dirs.push((system_root.join("System32"), SearchDirKind::System));
            dirs.push((system_root, SearchDirKind::System));
        }
    }

    dirs
}

/// Search directories for the macOS resolver, in priority order.
///
/// Deliberately compiled on Windows as well as macOS so that it type-checks and
/// can be exercised there; only the producer selection in
/// [`crate::dependencies::get_dependency_libs`] is macOS-gated.
#[cfg(any(windows, target_os = "macos"))]
pub fn macos_search_dirs(folder: &Path, install_plugins_dir: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    // First, because `install_name_tool -add_rpath @executable_path/lib` is
    // applied to the executable only *after* the folder is populated, so that
    // rpath does not exist while this scan runs.
    dirs.push(folder.join("lib"));

    if let Some(prefixes) = std::env::var_os("CMAKE_PREFIX_PATH") {
        for prefix in std::env::split_paths(&prefixes) {
            // Same order the packaging flow has always used on non-Windows.
            for relative in ["lib", "bin", "debug/lib", "debug/bin"] {
                let path = prefix.join(relative);
                if path.is_dir() {
                    dirs.push(path);
                }
            }
        }
    }

    if let Ok(entries) = std::fs::read_dir(install_plugins_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            }
        }
    }

    // Parity with what the old otool script consulted.
    for var in [
        "DYLD_LIBRARY_PATH",
        "DYLD_FALLBACK_LIBRARY_PATH",
        "DYLD_FRAMEWORK_PATH",
    ] {
        if let Some(value) = std::env::var_os(var) {
            for path in std::env::split_paths(&value) {
                if path.is_dir() {
                    dirs.push(path);
                }
            }
        }
    }

    let mut seen = std::collections::HashSet::new();
    dirs.retain(|dir| seen.insert(dir.clone()));
    for dir in &dirs {
        debug!("--> macOS search dir: {:?}", dir);
    }
    dirs
}

/// Everything needed to run a macOS walk.
#[cfg(any(windows, target_os = "macos"))]
pub struct MacosWalk {
    pub scanner: crate::macho_resolve::MachoScanner<'static>,
    pub folder_index: FolderIndex,
    pub roots: Vec<PathBuf>,
    pub env: ResolveEnv,
}

#[cfg(any(windows, target_os = "macos"))]
pub fn prepare_macos_walk(
    folder: &Path,
    main_exe: &Path,
    system_prefixes_path: Option<&Path>,
) -> Result<MacosWalk, anyhow::Error> {
    let system_prefixes = crate::macho_resolve::load_system_prefixes(system_prefixes_path);
    info!("macOS system library prefixes: {:?}", system_prefixes);
    let scanner = crate::macho_resolve::MachoScanner::new(system_prefixes);
    let folder_index = FolderIndex::build(folder, &scanner)?;

    let mut roots = vec![main_exe.to_path_buf()];
    for path in folder_index.paths() {
        if path != main_exe {
            roots.push(path);
        }
    }

    let env = ResolveEnv {
        search_dirs: macos_search_dirs(folder, &folder.join("Qt6/plugins")),
        executable_dir: main_exe.parent().unwrap_or(folder).to_path_buf(),
        output_dir: folder.to_path_buf(),
    };

    Ok(MacosWalk {
        scanner,
        folder_index,
        roots,
        env,
    })
}

/// Locate the application executable inside a package folder.
#[cfg(windows)]
pub fn find_main_executable(folder: &Path) -> Result<PathBuf, anyhow::Error> {
    let preferred = if cfg!(target_os = "windows") {
        folder.join("shoopdaloop_exe.exe")
    } else {
        folder.join("shoopdaloop_exe")
    };
    if preferred.is_file() {
        return Ok(preferred);
    }
    // Fall back to any executable at the folder root, so the command still works
    // on a layout that has been renamed.
    if cfg!(target_os = "windows") {
        for entry in std::fs::read_dir(folder)?.flatten() {
            let path = entry.path();
            if path.is_file()
                && path
                    .extension()
                    .map(|e| e.eq_ignore_ascii_case("exe"))
                    .unwrap_or(false)
            {
                return Ok(path);
            }
        }
    }
    Err(anyhow!(
        "Cannot find the application executable in {:?}",
        folder
    ))
}

/// Resolve the include/exclude list paths, defaulting to this platform's files.
#[cfg(windows)]
fn resolve_list_paths(options: &ScanOptions) -> Result<(PathBuf, PathBuf), anyhow::Error> {
    let platform = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let defaults = crate::fs_helpers::source_root()?
        .join("distribution")
        .join(platform);
    Ok((
        options
            .includelist
            .clone()
            .unwrap_or_else(|| defaults.join("includelist")),
        options
            .excludelist
            .clone()
            .unwrap_or_else(|| defaults.join("excludelist")),
    ))
}

/// Everything needed to run a Windows walk, assembled once so that the
/// packaging flow and the standalone `scan-dependencies` command cannot drift
/// apart in how they configure it.
#[cfg(windows)]
pub struct WindowsWalk {
    pub scanner: WindowsScanner,
    pub folder_index: FolderIndex,
    pub roots: Vec<PathBuf>,
    pub env: ResolveEnv,
    pub search_dirs: Vec<(PathBuf, SearchDirKind)>,
}

#[cfg(windows)]
pub fn prepare_windows_walk(
    folder: &Path,
    main_exe: &Path,
    extra_search_dirs: &[PathBuf],
    use_cmake_prefix_path: bool,
    no_system_dirs: bool,
    legacy_root_only: bool,
) -> Result<WindowsWalk, anyhow::Error> {
    let expected_machine = machine_of(main_exe);
    let search_dirs = windows_search_dirs(extra_search_dirs, use_cmake_prefix_path, no_system_dirs);
    let scanner = WindowsScanner::new(&search_dirs, expected_machine);
    let folder_index = FolderIndex::build(folder, &scanner)?;

    // The main executable goes first: the walker uses the first root's context
    // as the tail of every other root's loader chain.
    let roots = if legacy_root_only {
        vec![main_exe.to_path_buf()]
    } else {
        let mut roots = vec![main_exe.to_path_buf()];
        for path in folder_index.paths() {
            if path != main_exe {
                roots.push(path);
            }
        }
        roots
    };

    let env = ResolveEnv {
        search_dirs: search_dirs.iter().map(|(p, _)| p.clone()).collect(),
        executable_dir: main_exe.parent().unwrap_or(folder).to_path_buf(),
        output_dir: folder.to_path_buf(),
    };

    Ok(WindowsWalk {
        scanner,
        folder_index,
        roots,
        env,
        search_dirs,
    })
}

/// Environment variable that downgrades unresolved and unclassified
/// dependencies to warnings and prints a paste-ready block of suggested
/// includelist additions.
///
/// Seeding the walk from every binary in the package makes it reach Qt plugin
/// dependencies that have never been on any list, and each one is a hard error.
/// Rather than discovering them one failed CI run at a time, set this once on a
/// runner and harvest the whole set in a single round trip.
pub const REPORT_ONLY_VAR: &str = "SHOOP_PACKAGING_DEPS_REPORT_ONLY";

/// Print suggested includelist additions for everything that matched no list.
#[cfg(any(windows, target_os = "macos"))]
pub fn report_unlisted_candidates(report: &ScanReport) {
    if report.unclassified.is_empty() && report.unresolved.is_empty() {
        return;
    }
    let mut candidates: Vec<String> = report
        .unclassified
        .keys()
        .map(|key| format!("*/{key}"))
        .collect();
    candidates.sort();
    candidates.dedup();

    warn!(
        "{} dependencies matched neither list and {} could not be resolved.",
        report.unclassified.len(),
        report.unresolved.len()
    );
    if !candidates.is_empty() {
        println!("=== suggested includelist additions ===");
        for candidate in &candidates {
            println!("{candidate}");
        }
        println!("=== end suggested includelist additions ===");
        println!(
            "Review every line: each is a decision about what the application \
             ships. Anything the operating system provides belongs in the \
             excludelist (or, on macOS, in system_lib_prefixes) instead."
        );
    }
    for (key, _) in &report.unclassified {
        info!("  {key} needed by: {}", report.importers_summary(key));
    }
}

/// Log a one-line-per-category summary. Called on the packaging path, where
/// there is no report printed but the numbers still belong in the build log.
#[cfg(any(windows, target_os = "macos"))]
pub fn log_report_summary(report: &ScanReport) {
    info!(
        "Dependency scan: {} seeds, {} edges; {} already in folder, {} to copy, \
         {} excluded, {} OS-provided, {} unresolved, {} unclassified",
        report.seeds.len(),
        report.edges_walked,
        report.in_folder.len(),
        report.to_copy.len(),
        report.excluded.len(),
        report.provided.len(),
        report.unresolved.len(),
        report.unclassified.len(),
    );

    // Name what pulled in each C/C++ runtime.
    //
    // A package that ends up with both the debug and release Visual C++ runtime
    // crashes at startup, and the useful question is always "which dependency
    // asked for the other flavour". These libraries also import each other
    // (MSVCP140_2.dll imports MSVCP140.dll, VCRUNTIME140.dll and
    // VCRUNTIME140_1.dll), so a single mismatched binary drags in a whole chain
    // and only the first link identifies the culprit.
    for (key, entry) in &report.to_copy {
        let is_runtime = key.starts_with("msvcp") || key.starts_with("vcruntime");
        if is_runtime {
            info!(
                "  Runtime {} <- {} (from {})",
                key,
                report.importers_summary(key),
                entry.path.display()
            );
        }
    }
}

#[cfg(windows)]
pub fn run_scan(options: &ScanOptions) -> Result<ScanReport, anyhow::Error> {
    if !options.folder.is_dir() {
        return Err(anyhow!("Not a directory: {:?}", options.folder));
    }
    let (includelist, excludelist) = resolve_list_paths(options)?;
    info!("Include list: {:?}", includelist);
    info!("Exclude list: {:?}", excludelist);
    let matcher = ListMatcher::from_files(&includelist, &excludelist)?;

    let main_exe = find_main_executable(&options.folder)?;
    let walk = prepare_windows_walk(
        &options.folder,
        &main_exe,
        &options.extra_search_dirs,
        options.use_cmake_prefix_path,
        options.no_system_dirs,
        options.legacy_root_only,
    )?;

    let request = ScanRequest {
        roots: walk.roots.clone(),
        output_folder: options.folder.clone(),
        report_only: options.report_only,
        max_depth: options.max_depth,
    };

    let mut error_msgs = String::new();
    let (_tree, report) = build_dependency_tree(
        &walk.scanner,
        &request,
        &walk.env,
        &matcher,
        &walk.folder_index,
        &mut error_msgs,
    )?;

    log_report_summary(&report);
    print_report(
        &report,
        &walk.scanner.search_dir_summary(),
        &walk.folder_index,
    );
    if options.print_list_candidates {
        print_list_candidates(&report, &walk.search_dirs);
    }
    if !error_msgs.is_empty() {
        println!("\n--- errors ---\n{error_msgs}");
    }

    Ok(report)
}

#[cfg(not(windows))]
pub fn run_scan(_options: &ScanOptions) -> Result<ScanReport, anyhow::Error> {
    Err(anyhow!(
        "scan-dependencies is currently implemented for Windows only"
    ))
}

#[cfg(windows)]
fn print_report(
    report: &ScanReport,
    search_dirs: &[(PathBuf, SearchDirKind, usize)],
    folder_index: &FolderIndex,
) {
    println!();
    println!("Seeds: {} binaries", report.seeds.len());
    println!("Folder index: {} binaries", folder_index.len());
    if !report.skipped_non_binaries.is_empty() {
        println!(
            "Skipped (right extension, not a binary): {}",
            report.skipped_non_binaries.len()
        );
    }
    println!("Search dirs, in order:");
    for (index, (path, kind, count)) in search_dirs.iter().enumerate() {
        println!(
            "  {:>2} [{:?}] {} ({} files)",
            index + 1,
            kind,
            path.display(),
            count
        );
    }
    println!("Edges walked: {}", report.edges_walked);
    println!();
    println!(
        "IN FOLDER (traverse, not copied) .... {:>5}",
        report.in_folder.len()
    );
    println!(
        "TO COPY (matched includelist) ....... {:>5}",
        report.to_copy.len()
    );
    println!(
        "EXCLUDED BY NAME .................... {:>5}",
        report.excluded.len()
    );
    println!(
        "OS-PROVIDED (pruned) ................ {:>5}",
        report.provided.len()
    );
    println!(
        "UNRESOLVED (includelisted, no file) . {:>5}   <-- ERRORS",
        report.unresolved.len()
    );
    println!(
        "UNCLASSIFIED (matched neither list) . {:>5}   <-- ERRORS",
        report.unclassified.len()
    );

    print_problem_section(
        "UNRESOLVED (matches the includelist but no file was found anywhere)",
        &report.unresolved,
        report,
    );
    print_problem_section(
        "UNCLASSIFIED (matched neither list)",
        &report.unclassified,
        report,
    );

    if !report.excluded.is_empty() {
        println!("\n--- EXCLUDED BY NAME ---");
        for (key, pattern) in &report.excluded {
            println!("  {key}  [{pattern}]");
        }
    }
    if !report.to_copy.is_empty() {
        println!("\n--- TO COPY ---");
        for (key, entry) in &report.to_copy {
            println!(
                "  {key}  [{}]  <- {}",
                entry.matched_pattern,
                entry.path.display()
            );
        }
    }
}

#[cfg(windows)]
fn print_problem_section(title: &str, items: &BTreeMap<String, Missing>, report: &ScanReport) {
    if items.is_empty() {
        return;
    }
    println!("\n--- {title} ---");
    for (key, missing) in items {
        let where_ = match &missing.nominal {
            Some(p) => format!("resolved {}", p.display()),
            None => String::from("NOT FOUND"),
        };
        let pattern = missing
            .matched_pattern
            .as_deref()
            .map(|p| format!("  [{p}]"))
            .unwrap_or_default();
        println!("  {key}{pattern}  {where_}");
        println!("      needed by: {}", report.importers_summary(key));
    }
}

/// Emit paste-ready list entries, split by whether the dependency came from a
/// system directory.
///
/// The split matters: an excludelist entry is a claim that the operating system
/// provides the library, which is mechanically checkable and safe to accept in
/// bulk. An includelist entry is a decision about what the application ships,
/// and every one needs a human.
#[cfg(windows)]
fn print_list_candidates(report: &ScanReport, search_dirs: &[(PathBuf, SearchDirKind)]) {
    let system_dirs: Vec<&PathBuf> = search_dirs
        .iter()
        .filter(|(_, kind)| *kind == SearchDirKind::System)
        .map(|(path, _)| path)
        .collect();
    let from_system = |missing: &Missing| -> bool {
        missing
            .nominal
            .as_ref()
            .map(|p| system_dirs.iter().any(|d| p.starts_with(d)))
            .unwrap_or(false)
    };

    let mut exclude_candidates: Vec<String> = Vec::new();
    let mut include_candidates: Vec<String> = Vec::new();
    for (key, missing) in &report.unclassified {
        if from_system(missing) {
            exclude_candidates.push(format!("*/{key}"));
        } else {
            include_candidates.push(format!("*/{key}"));
        }
    }
    exclude_candidates.sort();
    include_candidates.sort();

    println!("\n### excludelist candidates (resolved from a system directory) ###");
    for line in &exclude_candidates {
        println!("{line}");
    }
    println!("\n### includelist candidates (NOT from a system directory - REVIEW EACH) ###");
    for line in &include_candidates {
        println!("{line}");
    }
}

/// Tests against a real package folder.
///
/// A machine-specific absolute path cannot be committed, so these are gated on
/// `SHOOP_TEST_PACKAGE_DIR` and skip when it is unset:
///
/// ```text
/// SHOOP_TEST_PACKAGE_DIR=<path to an extracted portable folder> cargo test -p packaging
/// ```
///
/// Assertions are deliberately structural rather than exact counts, so they
/// survive being pointed at a different build of the package.
#[cfg(all(test, windows))]
mod real_package_tests {
    use super::*;
    use crate::deps_walker::build_dependency_tree;

    fn package_dir() -> Option<PathBuf> {
        let dir = PathBuf::from(std::env::var_os("SHOOP_TEST_PACKAGE_DIR")?);
        if !dir.is_dir() {
            eprintln!("SHOOP_TEST_PACKAGE_DIR is not a directory: {dir:?}");
            return None;
        }
        Some(dir)
    }

    fn source_list_paths() -> (PathBuf, PathBuf) {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../distribution/windows");
        (dir.join("includelist"), dir.join("excludelist"))
    }

    #[test]
    fn scans_a_real_package_and_finds_the_qml_plugin_dependencies() {
        let Some(folder) = package_dir() else {
            eprintln!("skipping: SHOOP_TEST_PACKAGE_DIR not set");
            return;
        };
        let (includelist, excludelist) = source_list_paths();
        let matcher = ListMatcher::from_files(&includelist, &excludelist).unwrap();
        let main_exe = find_main_executable(&folder).expect("executable in package");

        let walk = prepare_windows_walk(&folder, &main_exe, &[], false, false, false).unwrap();
        let request = ScanRequest {
            roots: walk.roots.clone(),
            output_folder: folder.clone(),
            report_only: true,
            max_depth: None,
        };
        let mut errors = String::new();
        let (_tree, report) = build_dependency_tree(
            &walk.scanner,
            &request,
            &walk.env,
            &matcher,
            &walk.folder_index,
            &mut errors,
        )
        .unwrap();

        assert!(
            report.seeds.len() >= 100,
            "expected the whole package to be seeded, got {} seeds",
            report.seeds.len()
        );
        // API-set classification must fire, or the walk would be full of
        // unresolvable references to libraries that have no file anywhere.
        assert!(!report.provided.is_empty(), "no API sets were classified");
        assert!(
            report.excluded.contains_key("kernel32.dll"),
            "kernel32 should be excluded by name"
        );

        // The regression assertion for the bug this all exists to fix. On a
        // finished package the Qt libraries are absent, so this lands in
        // `unresolved`; in a real build with vcpkg present it lands in `to_copy`.
        let key = "qt6quickcontrols2.dll";
        assert!(
            report.unresolved.contains_key(key) || report.to_copy.contains_key(key),
            "Qt6QuickControls2.dll must be discovered as a dependency"
        );
        let importers = report
            .importers
            .get(key)
            .expect("importers must be attributed");
        assert!(
            importers
                .iter()
                .any(|i| i.contains("qtquickcontrols2") && i.contains("plugin")),
            "expected a QtQuick.Controls plugin among importers, got {importers:?}"
        );
    }

    /// The delay-load import table is a separate PE data directory that goblin
    /// does not parse, so it is walked by hand. `dbghelp.dll -> rpcrt4.dll` is
    /// the only delay-load edge in the package, which makes it the only
    /// available regression target -- but a real one.
    #[test]
    fn delay_load_imports_are_parsed() {
        let Some(folder) = package_dir() else {
            eprintln!("skipping: SHOOP_TEST_PACKAGE_DIR not set");
            return;
        };
        let dbghelp = folder.join("lib").join("dbghelp.dll");
        if !dbghelp.is_file() {
            eprintln!("skipping: {dbghelp:?} not present in this package");
            return;
        }
        let imports = crate::pe::read_pe_imports(&dbghelp)
            .expect("must parse")
            .expect("dbghelp.dll is a PE image");

        assert!(
            imports
                .delay_libraries
                .iter()
                .any(|l| l.eq_ignore_ascii_case("rpcrt4.dll")),
            "expected rpcrt4.dll among delay imports, got {:?}",
            imports.delay_libraries
        );
        // It must NOT appear in the normal import table, otherwise this test
        // would pass even with the delay-load parser removed.
        assert!(
            !imports
                .libraries
                .iter()
                .any(|l| l.eq_ignore_ascii_case("rpcrt4.dll")),
            "rpcrt4.dll should only be reachable as a delay import"
        );
    }
}
