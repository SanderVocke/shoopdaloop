//! macOS dependency resolution: `@rpath` / `@loader_path` / `@executable_path`
//! expansion, system-library classification, and framework reduction.
//!
//! # Why paths are handled as strings
//!
//! Every candidate path is built by joining POSIX-shaped `String`s with `/`,
//! and converted to a `PathBuf` only when a [`Resolution`] is constructed. On a
//! Windows host `PathBuf::from("/opt/lib").join("libfoo.dylib")` yields
//! `/opt/lib\libfoo.dylib`, which would make every candidate list depend on the
//! host OS and the whole module untestable anywhere but macOS. Modelling macOS
//! paths as strings keeps the semantics byte-exact on any host, which is what
//! lets the tests below run on the developer's machine.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::deps_walker::{
    BinaryScanner, LibraryReference, Resolution, ResolveEnv, ScannedBinary,
};
use crate::macho::{is_macho_file, read_macho_file};

use common::logging::macros::*;
shoop_log_unit!("packaging");

/// Path prefixes for libraries macOS itself provides.
///
/// Used when `distribution/macos/system_lib_prefixes` cannot be read.
pub const DEFAULT_SYSTEM_PREFIXES: &[&str] = &["/usr/lib/", "/System/", "/Library/Apple/"];

/// Per-binary context needed to resolve that binary's references.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MachoContext {
    /// POSIX directory containing the binary that declares the references.
    pub binary_dir: String,
    /// That binary's `LC_RPATH` entries, verbatim.
    pub rpaths: Vec<String>,
}

/// Convert a path to a POSIX-shaped string.
pub fn to_posix(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Join with `/`, collapsing duplicate separators and `./` segments.
///
/// `..` is deliberately left in place: dyld does not perform lexical
/// normalisation either, and removing `..` lexically is wrong when a symlink is
/// involved.
pub fn posix_join(dir: &str, rest: &str) -> String {
    let dir = dir.trim_end_matches('/');
    let rest = rest.trim_start_matches('/');
    let joined = if dir.is_empty() {
        rest.to_string()
    } else {
        format!("{dir}/{rest}")
    };
    // Collapse "//" and "/./" without touching a leading "/".
    let mut out = String::with_capacity(joined.len());
    let mut chars = joined.chars().peekable();
    let leading_slash = joined.starts_with('/');
    while let Some(c) = chars.next() {
        if c == '/' {
            while chars.peek() == Some(&'/') {
                chars.next();
            }
            out.push('/');
        } else {
            out.push(c);
        }
    }
    let collapsed = out
        .split('/')
        .enumerate()
        .filter(|(index, segment)| !(*segment == "." && *index > 0))
        .map(|(_, segment)| segment)
        .collect::<Vec<_>>()
        .join("/");
    if leading_slash && !collapsed.starts_with('/') {
        format!("/{}", collapsed.trim_start_matches('/'))
    } else {
        collapsed
    }
}

/// Expand one `LC_RPATH` entry to an absolute directory.
///
/// `None` for entries that cannot be made absolute -- a bare relative rpath,
/// which dyld would resolve against the process's working directory. That is
/// never meaningful for packaging, so the caller skips it rather than joining it
/// to something arbitrary.
///
/// Note `@executable_path` is handled here. The old shell script substituted
/// only `@loader_path` in rpath entries, and did so using the *root
/// executable's* directory rather than the declaring binary's.
pub fn expand_rpath_entry(
    entry: &str,
    loader_dir: &str,
    executable_dir: &str,
) -> Option<String> {
    if let Some(rest) = entry.strip_prefix("@loader_path") {
        return Some(posix_join(loader_dir, rest));
    }
    if let Some(rest) = entry.strip_prefix("@executable_path") {
        return Some(posix_join(executable_dir, rest));
    }
    if entry.starts_with('/') {
        return Some(entry.trim_end_matches('/').to_string());
    }
    None
}

/// Candidate paths for `raw`, in the order they should be tried. No filesystem
/// access, which is what makes this the module's main test surface.
///
/// `chain[0]` declares the reference; later entries are its loaders, nearest
/// first. dyld accumulates run-paths along that chain, so `@rpath` is tried
/// against each binary's rpaths in that order.
pub fn resolution_candidates(
    raw: &str,
    chain: &[MachoContext],
    executable_dir: &str,
    search_dirs: &[String],
) -> Vec<String> {
    let mut candidates: Vec<String> = Vec::new();
    let mut push = |candidate: String| {
        if !candidate.is_empty() && !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    };

    let loader_dir = chain
        .first()
        .map(|c| c.binary_dir.clone())
        .unwrap_or_default();

    // The remainder after any `@`-prefix, used both for the prefix cases and for
    // the search-directory fallback.
    let mut remainder: Option<&str> = None;

    if let Some(rest) = raw.strip_prefix("@executable_path/") {
        remainder = Some(rest);
        push(posix_join(executable_dir, rest));
    } else if let Some(rest) = raw.strip_prefix("@loader_path/") {
        remainder = Some(rest);
        push(posix_join(&loader_dir, rest));
    } else if let Some(rest) = raw.strip_prefix("@rpath/") {
        remainder = Some(rest);
        for context in chain {
            for entry in &context.rpaths {
                if let Some(dir) = expand_rpath_entry(entry, &context.binary_dir, executable_dir)
                {
                    push(posix_join(&dir, rest));
                } else {
                    debug!(
                        "  Ignoring relative LC_RPATH entry {:?} (dyld would resolve it \
                         against the working directory)",
                        entry
                    );
                }
            }
        }
    } else if raw.starts_with('/') {
        push(raw.to_string());
    } else {
        // A bare relative install name: dyld looks next to the loader.
        remainder = Some(raw);
        push(posix_join(&loader_dir, raw));
    }

    // Fallback: the explicit search directories.
    //
    // This is deliberately NOT dyld semantics, and it is load-bearing rather than
    // a convenience. Three reasons:
    //
    //  * `install_name_tool -add_rpath @executable_path/lib` runs on the
    //    executable only *after* the folder is populated, so that rpath does not
    //    exist while the scan runs.
    //  * The newly-seeded roots are Qt plugins whose own `LC_RPATH` points into
    //    Qt's build prefix, or at a `@loader_path/../../lib` that does not exist
    //    in the bundle layout.
    //  * The vcpkg library directories are known to the packager and are exactly
    //    where the files are.
    //
    // A hit here is reported by the caller at warning level, because it means the
    // runtime layout may not match what was bundled.
    let rest = remainder.unwrap_or(raw);
    for dir in search_dirs {
        push(posix_join(dir, rest));
        if rest.contains('/') {
            if let Some(base) = rest.rsplit('/').next() {
                push(posix_join(dir, base));
            }
        }
    }

    candidates
}

/// The outermost enclosing `*.framework` directory, or `path` unchanged.
///
/// Replaces a regex (`(.*/.*.framework)/.*`) that had three defects: the `.`
/// before `framework` was unescaped so `QtCoreXframework` matched; it was greedy
/// and unanchored, so for nested frameworks it selected the *innermost*; and it
/// used `Regex::new(..).expect(..)`, so a bad pattern would panic.
///
/// Outermost is the correct choice. A nested
/// `Foo.framework/Frameworks/Bar.framework/Bar` is referenced from `Foo` by a
/// relative path, so copying only `Bar.framework` out of it breaks that layout;
/// copying `Foo.framework` preserves it.
pub fn reduce_to_framework_root(path: &Path) -> PathBuf {
    let mut outermost: Option<&Path> = None;
    for ancestor in path.ancestors() {
        let is_framework = ancestor
            .file_name()
            .map(|name| name.to_string_lossy().ends_with(".framework"))
            .unwrap_or(false);
        if is_framework {
            outermost = Some(ancestor);
        }
    }
    outermost.map(Path::to_path_buf).unwrap_or_else(|| path.to_path_buf())
}

/// Read the system-library prefixes from a data file, falling back to the
/// built-in defaults.
pub fn load_system_prefixes(path: Option<&Path>) -> Vec<String> {
    let Some(path) = path else {
        return DEFAULT_SYSTEM_PREFIXES.iter().map(|s| s.to_string()).collect();
    };
    match std::fs::read_to_string(path) {
        Ok(body) => {
            let prefixes: Vec<String> = body
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .map(|line| line.to_string())
                .collect();
            if prefixes.is_empty() {
                warn!(
                    "  {:?} contains no prefixes; using built-in defaults",
                    path
                );
                return DEFAULT_SYSTEM_PREFIXES.iter().map(|s| s.to_string()).collect();
            }
            prefixes
        }
        Err(e) => {
            warn!(
                "  Cannot read {:?} ({e}); using built-in system library prefixes",
                path
            );
            DEFAULT_SYSTEM_PREFIXES.iter().map(|s| s.to_string()).collect()
        }
    }
}

/// The macOS [`BinaryScanner`].
pub struct MachoScanner<'a> {
    system_prefixes: Vec<String>,
    /// Injected so the whole resolver is testable against a fake filesystem.
    exists: Box<dyn Fn(&str) -> bool + 'a>,
}

impl MachoScanner<'static> {
    pub fn new(system_prefixes: Vec<String>) -> Self {
        Self {
            system_prefixes,
            exists: Box::new(|candidate: &str| Path::new(candidate).is_file()),
        }
    }
}

impl<'a> MachoScanner<'a> {
    pub fn with_exists(
        system_prefixes: Vec<String>,
        exists: Box<dyn Fn(&str) -> bool + 'a>,
    ) -> Self {
        Self {
            system_prefixes,
            exists,
        }
    }

    fn is_system_path(&self, candidate: &str) -> bool {
        self.system_prefixes
            .iter()
            .any(|prefix| candidate.starts_with(prefix.as_str()))
    }
}

fn basename_key(raw: &str) -> String {
    raw.rsplit('/').next().unwrap_or(raw).to_lowercase()
}

impl BinaryScanner for MachoScanner<'_> {
    type Context = MachoContext;

    fn format_name(&self) -> &'static str {
        "Mach-O"
    }

    fn is_binary(&self, path: &Path) -> bool {
        is_macho_file(path)
    }

    fn scan(
        &self,
        binary: &Path,
    ) -> Result<Option<ScannedBinary<MachoContext>>, anyhow::Error> {
        let Some(info) = read_macho_file(binary)? else {
            return Ok(None);
        };
        let binary_dir = binary
            .parent()
            .map(to_posix)
            .unwrap_or_default();
        Ok(Some(ScannedBinary {
            references: info.libs.iter().map(LibraryReference::new).collect(),
            context: MachoContext {
                binary_dir,
                rpaths: info.rpaths,
            },
        }))
    }

    fn reference_key(&self, reference: &LibraryReference) -> String {
        basename_key(&reference.raw)
    }

    fn path_key(&self, path: &Path) -> String {
        path.file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    }

    fn provided_reason(&self, reference: &LibraryReference) -> Option<String> {
        // Prefix, not existence. On macOS 11 and later most of these live only
        // in the dyld shared cache and have no file on disk, while on older
        // versions they do -- so asking "does the file exist" gives a
        // version-dependent answer to a question that has a fixed one. Using
        // existence as the criterion would also silently prune a genuinely
        // missing vcpkg dylib, which is the failure mode that shipped the broken
        // package in the first place.
        if self.is_system_path(&reference.raw) {
            return Some(format!(
                "provided by macOS (matches a system library prefix): {}",
                reference.raw
            ));
        }
        None
    }

    fn resolve(
        &self,
        reference: &LibraryReference,
        chain: &[MachoContext],
        env: &ResolveEnv,
    ) -> Resolution {
        let executable_dir = to_posix(&env.executable_dir);
        let search_dirs: Vec<String> = env.search_dirs.iter().map(|p| to_posix(p)).collect();
        let candidates =
            resolution_candidates(&reference.raw, chain, &executable_dir, &search_dirs);

        // Where the reference's own semantics stop and the search-directory
        // fallback begins, so that a fallback hit can be reported as such.
        //
        // Computed by re-deriving the candidate list with no search directories
        // rather than by subtracting their count: each search directory can
        // contribute up to two candidates, and duplicates are removed, so any
        // arithmetic on `search_dirs.len()` would be wrong. The semantic
        // candidates are the prefix of the full list, so their count is the
        // boundary.
        let semantic_count =
            resolution_candidates(&reference.raw, chain, &executable_dir, &[]).len();

        for (index, candidate) in candidates.iter().enumerate() {
            if !(self.exists)(candidate) {
                continue;
            }
            if self.is_system_path(candidate) {
                // Reached only when the reference itself was not a system path
                // (that is pruned earlier) but resolved into one.
                debug!(
                    "  {} resolved into a system location {}; not bundling",
                    reference.raw, candidate
                );
                continue;
            }
            if index >= semantic_count {
                warn!(
                    "  {} was only found via a search directory ({}). The bundled \
                     layout may not match what the loader will look for at runtime.",
                    reference.raw, candidate
                );
            }
            return Resolution::Found(PathBuf::from(candidate));
        }

        if reference.raw.starts_with('/') {
            return Resolution::Nominal(PathBuf::from(&reference.raw));
        }
        Resolution::Unresolvable {
            tried: candidates.into_iter().map(PathBuf::from).collect(),
        }
    }
}

/// Reduce a set of resolved library paths so that files inside a framework are
/// replaced by the framework directory itself.
///
/// Applied to the final set at copy time only, never during traversal: the walk
/// resolves to the inner binary and parses that, which is how it descends into
/// frameworks correctly. Include/exclude matching also stays on the inner path,
/// since the committed patterns are written against library file names.
pub fn reduce_framework_paths(paths: HashSet<PathBuf>) -> HashSet<PathBuf> {
    paths.into_iter().map(|p| reduce_to_framework_root(&p)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|s| s.to_string()).collect()
    }

    fn context(dir: &str, rpaths: &[&str]) -> MachoContext {
        MachoContext {
            binary_dir: dir.to_string(),
            rpaths: rpaths.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn scanner_with(existing: &[&str]) -> MachoScanner<'static> {
        let set: HashSet<String> = existing.iter().map(|s| s.to_string()).collect();
        MachoScanner::with_exists(
            DEFAULT_SYSTEM_PREFIXES.iter().map(|s| s.to_string()).collect(),
            Box::new(move |candidate| set.contains(candidate)),
        )
    }

    fn env(search_dirs: &[&str], executable_dir: &str) -> ResolveEnv {
        ResolveEnv {
            search_dirs: search_dirs.iter().map(PathBuf::from).collect(),
            executable_dir: PathBuf::from(executable_dir),
            output_dir: PathBuf::from(executable_dir),
        }
    }

    #[test]
    fn posix_join_collapses_separators_but_keeps_dotdot() {
        assert_eq!(posix_join("/a/b", "c.dylib"), "/a/b/c.dylib");
        assert_eq!(posix_join("/a/b/", "/c.dylib"), "/a/b/c.dylib");
        assert_eq!(posix_join("/a//b", "c.dylib"), "/a/b/c.dylib");
        assert_eq!(posix_join("/a/b", "./c.dylib"), "/a/b/c.dylib");
        // dyld does not normalise `..`, and doing it lexically is wrong across
        // symlinks.
        assert_eq!(posix_join("/a/b", "../lib/c.dylib"), "/a/b/../lib/c.dylib");
    }

    #[test]
    fn executable_path_is_expanded() {
        let c = resolution_candidates(
            "@executable_path/lib/libfoo.dylib",
            &[context("/pkg/Qt6/qml/X", &[])],
            "/pkg",
            &[],
        );
        assert_eq!(c, vec!["/pkg/lib/libfoo.dylib"]);
    }

    #[test]
    fn loader_path_is_expanded_against_the_declaring_binary() {
        // The old script expanded @loader_path using the ROOT executable's
        // directory, which is wrong for anything but the executable itself.
        let c = resolution_candidates(
            "@loader_path/../../lib/libfoo.dylib",
            &[context("/pkg/Qt6/qml/X", &[])],
            "/pkg",
            &[],
        );
        assert_eq!(c, vec!["/pkg/Qt6/qml/X/../../lib/libfoo.dylib"]);
    }

    /// dyld accumulates run-paths from the loaded image up towards the main
    /// executable, so the declaring binary's rpaths must be tried first.
    #[test]
    fn rpath_uses_the_whole_chain_nearest_first() {
        let c = resolution_candidates(
            "@rpath/libQt6Core.6.dylib",
            &[
                context("/pkg/Qt6/qml/X", &["@loader_path/../lib"]),
                context("/pkg", &["/opt/vcpkg/lib"]),
            ],
            "/pkg",
            &[],
        );
        assert_eq!(
            c,
            vec![
                "/pkg/Qt6/qml/X/../lib/libQt6Core.6.dylib",
                "/opt/vcpkg/lib/libQt6Core.6.dylib",
            ]
        );
    }

    /// `@executable_path` inside an LC_RPATH entry was never substituted by the
    /// old script.
    #[test]
    fn executable_path_inside_an_rpath_entry_is_expanded() {
        let c = resolution_candidates(
            "@rpath/libfoo.dylib",
            &[context("/pkg/plugins", &["@executable_path/lib"])],
            "/pkg",
            &[],
        );
        assert_eq!(c, vec!["/pkg/lib/libfoo.dylib"]);
    }

    #[test]
    fn a_bare_relative_rpath_entry_is_skipped_not_joined_to_the_cwd() {
        let c = resolution_candidates(
            "@rpath/libfoo.dylib",
            &[context("/pkg/plugins", &["relative/dir", "/absolute/dir"])],
            "/pkg",
            &[],
        );
        assert_eq!(
            c,
            vec!["/absolute/dir/libfoo.dylib"],
            "a relative rpath must not contribute a candidate"
        );
    }

    #[test]
    fn search_dirs_are_a_fallback_and_try_the_basename_too() {
        let c = resolution_candidates(
            "@rpath/subdir/libfoo.dylib",
            &[context("/pkg/plugins", &[])],
            "/pkg",
            &dirs(&["/pkg/lib", "/opt/vcpkg/lib"]),
        );
        assert_eq!(
            c,
            vec![
                "/pkg/lib/subdir/libfoo.dylib",
                "/pkg/lib/libfoo.dylib",
                "/opt/vcpkg/lib/subdir/libfoo.dylib",
                "/opt/vcpkg/lib/libfoo.dylib",
            ]
        );
    }

    #[test]
    fn an_absolute_reference_is_its_own_candidate() {
        let c = resolution_candidates(
            "/opt/homebrew/lib/libjack.dylib",
            &[context("/pkg", &[])],
            "/pkg",
            &[],
        );
        assert_eq!(c, vec!["/opt/homebrew/lib/libjack.dylib"]);
    }

    #[test]
    fn first_existing_candidate_wins() {
        let scanner = scanner_with(&["/opt/vcpkg/lib/libQt6Core.6.dylib"]);
        let resolution = scanner.resolve(
            &LibraryReference::new("@rpath/libQt6Core.6.dylib"),
            &[context("/pkg/plugins", &["/nowhere", "/opt/vcpkg/lib"])],
            &env(&[], "/pkg"),
        );
        assert_eq!(
            resolution,
            Resolution::Found(PathBuf::from("/opt/vcpkg/lib/libQt6Core.6.dylib"))
        );
    }

    /// This is why macOS builds pass today despite an empty excludelist: system
    /// libraries live in the dyld shared cache and have no file, so the old
    /// script silently dropped them before they reached the list check.
    #[test]
    fn a_missing_system_library_is_provided_not_an_error() {
        let scanner = scanner_with(&[]);
        let reason = scanner.provided_reason(&LibraryReference::new(
            "/usr/lib/libSystem.B.dylib",
        ));
        assert!(reason.is_some(), "must be classified as OS-provided");
    }

    /// Prefix beats existence: on older macOS these files do exist on disk, and
    /// bundling them would still be wrong.
    #[test]
    fn an_existing_system_library_is_still_provided() {
        let scanner = scanner_with(&["/usr/lib/libz.1.dylib"]);
        assert!(scanner
            .provided_reason(&LibraryReference::new("/usr/lib/libz.1.dylib"))
            .is_some());
    }

    /// The reason the system policy cannot live in the excludelist: the
    /// includelist's `*/libz*.dylib` matches `/usr/lib/libz.1.dylib`, and the
    /// includelist wins over the excludelist. Only a separate higher-precedence
    /// rule can distinguish the system copy from vcpkg's.
    #[test]
    fn includelist_patterns_do_match_system_paths() {
        let matcher =
            crate::list_matcher::ListMatcher::from_lines(&["*/libz*.dylib"], &["/usr/lib/*"])
                .unwrap();
        assert!(
            matcher.matched_include("/usr/lib/libz.1.dylib").is_some(),
            "if this ever stops matching, the system-prefix mechanism could be \
             replaced by excludelist entries"
        );
        assert!(matcher.matched_exclude("/usr/lib/libz.1.dylib").is_some());
    }

    /// Homebrew is not the system: libraries found there must be bundled.
    #[test]
    fn homebrew_and_usr_local_are_not_system_locations() {
        let scanner = scanner_with(&[]);
        for path in [
            "/usr/local/lib/libjack.dylib",
            "/opt/homebrew/lib/libfoo.dylib",
        ] {
            assert!(
                scanner
                    .provided_reason(&LibraryReference::new(path))
                    .is_none(),
                "{path} must not be treated as OS-provided"
            );
        }
    }

    #[test]
    fn an_unresolvable_rpath_reports_what_was_tried() {
        let scanner = scanner_with(&[]);
        let resolution = scanner.resolve(
            &LibraryReference::new("@rpath/libMissing.dylib"),
            &[context("/pkg/plugins", &["/opt/vcpkg/lib"])],
            &env(&["/pkg/lib"], "/pkg"),
        );
        match resolution {
            Resolution::Unresolvable { tried } => {
                assert!(tried.contains(&PathBuf::from("/opt/vcpkg/lib/libMissing.dylib")));
                assert!(tried.contains(&PathBuf::from("/pkg/lib/libMissing.dylib")));
            }
            other => panic!("expected Unresolvable, got {other:?}"),
        }
    }

    #[test]
    fn an_absolute_missing_non_system_reference_is_nominal() {
        let scanner = scanner_with(&[]);
        let resolution = scanner.resolve(
            &LibraryReference::new("/opt/vcpkg/lib/libGone.dylib"),
            &[context("/pkg", &[])],
            &env(&[], "/pkg"),
        );
        assert_eq!(
            resolution,
            Resolution::Nominal(PathBuf::from("/opt/vcpkg/lib/libGone.dylib"))
        );
    }

    #[test]
    fn framework_reduction_picks_the_outermost() {
        assert_eq!(
            reduce_to_framework_root(Path::new(
                "/opt/q/lib/QtCore.framework/Versions/A/QtCore"
            )),
            PathBuf::from("/opt/q/lib/QtCore.framework")
        );
        // Nested: the inner framework is referenced relative to the outer one,
        // so copying only the inner would break the layout.
        assert_eq!(
            reduce_to_framework_root(Path::new(
                "/o/Foo.framework/Frameworks/Bar.framework/Versions/A/Bar"
            )),
            PathBuf::from("/o/Foo.framework")
        );
    }

    #[test]
    fn framework_reduction_requires_a_literal_dot() {
        // The old regex left `.` unescaped, so this matched.
        assert_eq!(
            reduce_to_framework_root(Path::new("/x/QtCoreXframework/y/libz.dylib")),
            PathBuf::from("/x/QtCoreXframework/y/libz.dylib")
        );
        assert_eq!(
            reduce_to_framework_root(Path::new("/x/lib/libfoo.dylib")),
            PathBuf::from("/x/lib/libfoo.dylib")
        );
    }

    #[test]
    fn reference_keys_strip_the_at_prefix() {
        let scanner = scanner_with(&[]);
        assert_eq!(
            scanner.reference_key(&LibraryReference::new("@rpath/libQt6Core.6.dylib")),
            "libqt6core.6.dylib"
        );
        assert_eq!(
            scanner.reference_key(&LibraryReference::new("/usr/lib/libSystem.B.dylib")),
            "libsystem.b.dylib"
        );
        assert_eq!(
            scanner.reference_key(&LibraryReference::new("libfoo.dylib")),
            "libfoo.dylib"
        );
    }

    #[test]
    fn system_prefixes_fall_back_when_the_file_is_unreadable() {
        let prefixes = load_system_prefixes(Some(Path::new("/definitely/not/here")));
        assert_eq!(prefixes, DEFAULT_SYSTEM_PREFIXES);
    }

    /// Every prefix in the committed data file must be absolute and end in `/`,
    /// otherwise `/usr/libfoo` would be treated as being under `/usr/lib`.
    #[test]
    fn committed_system_prefixes_are_well_formed() {
        let body = include_str!("../../../../distribution/macos/system_lib_prefixes");
        let prefixes: Vec<&str> = body
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect();
        assert!(!prefixes.is_empty(), "the file must list some prefixes");
        for prefix in &prefixes {
            assert!(prefix.starts_with('/'), "{prefix} must be absolute");
            assert!(prefix.ends_with('/'), "{prefix} must end with a slash");
        }
        // The case that makes this whole mechanism necessary.
        let scanner = MachoScanner::with_exists(
            prefixes.iter().map(|s| s.to_string()).collect(),
            Box::new(|_| false),
        );
        assert!(scanner
            .provided_reason(&LibraryReference::new("/usr/lib/libSystem.B.dylib"))
            .is_some());
    }
}
