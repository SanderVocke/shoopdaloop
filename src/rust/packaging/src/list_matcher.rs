//! Include/exclude list matching for dependency classification.
//!
//! The pattern language is the one the `distribution/*/{include,exclude}list`
//! files have always used: shell-ish globs matched against a path, e.g.
//! `*/kernel32.dll`. Patterns are translated to regexes and matched
//! **unanchored**, exactly as [`dependencies::get_dependency_libs`] did inline
//! before this module existed.
//!
//! Two things are new here:
//!
//! * Patterns are compiled once at load time instead of once per candidate per
//!   node. A malformed pattern is now reported against the file it came from
//!   rather than mid-traversal.
//! * Blank lines and `#` comments are skipped. A blank line used to compile to
//!   the empty regex, which matches *everything* — an excludelist with a stray
//!   trailing blank line would have silently excluded every dependency.

use anyhow::Context;
use std::path::{Path, PathBuf};

use common::logging::macros::*;
shoop_log_unit!("packaging");

/// Synthetic directory used to give a bare library name a path shape.
///
/// List entries are *path* patterns: `*/kernel32.dll` compiles to the
/// unanchored regex `.*/kernel32\.dll`, which requires a `/`. A bare name such
/// as `kernel32.dll` therefore matches nothing, and every unresolved dependency
/// would fall through to "is not in include list" and fail the build. Prefixing
/// with a synthetic directory makes `.*` absorb the prefix and the pattern
/// match as intended.
///
/// The name is chosen to contain no character that survives
/// [`pattern_to_regex`] as a metacharacter, and to be impossible to confuse
/// with a real directory.
pub const UNRESOLVED_DIR: &str = "<unresolved>";

/// Path shape for a library name that could not be resolved to a real file.
pub fn synthetic_unresolved_path(name: &str) -> PathBuf {
    PathBuf::from(UNRESOLVED_DIR).join(name)
}

/// Normalise a path for matching: forward slashes, lowercased.
pub fn normalize_path_for_matching(path: &str) -> String {
    path.replace('\\', "/").to_lowercase()
}

/// Translate a list pattern into a regex source string.
///
/// The replacement order is load-bearing and reproduces the historical
/// behaviour exactly:
///
/// 1. `\` -> `\\`   (a literal backslash in the pattern)
/// 2. `.` -> `\.`   (literal dot)
/// 3. `*` -> `.*`   **after** step 2, so the `.` introduced here stays a wildcard
/// 4. `+` -> `\+`
/// 5. lowercase
///
/// Reordering 2 and 3 would turn every `*` into a literal-dot-plus-star and
/// silently stop matching.
fn pattern_to_regex(pattern: &str) -> String {
    pattern
        .replace('\\', "\\\\")
        .replace('.', "\\.")
        .replace('*', ".*")
        .replace('+', "\\+")
        .to_lowercase()
}

struct CompiledPattern {
    /// The pattern as written in the list file, for reporting.
    pattern: String,
    regex: regex::Regex,
}

impl CompiledPattern {
    fn compile(pattern: &str) -> Result<Self, anyhow::Error> {
        let source = pattern_to_regex(pattern);
        let regex = regex::Regex::new(&source).with_context(|| {
            format!("Invalid pattern {pattern:?} (translated to regex {source:?})")
        })?;
        Ok(Self {
            pattern: pattern.to_string(),
            regex,
        })
    }
}

/// Parse a list file body into patterns, skipping blanks and `#` comments.
fn parse_list(body: &str) -> Vec<&str> {
    body.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

/// Compiled include and exclude lists.
pub struct ListMatcher {
    includes: Vec<CompiledPattern>,
    excludes: Vec<CompiledPattern>,
}

impl ListMatcher {
    pub fn from_files(
        includelist_path: &Path,
        excludelist_path: &Path,
    ) -> Result<Self, anyhow::Error> {
        let includelist = std::fs::read_to_string(includelist_path)
            .with_context(|| format!("Cannot read {includelist_path:?}"))?;
        let excludelist = std::fs::read_to_string(excludelist_path)
            .with_context(|| format!("Cannot read {excludelist_path:?}"))?;

        let matcher = Self::from_lines(&parse_list(&includelist), &parse_list(&excludelist))
            .with_context(|| {
                format!("Compiling {includelist_path:?} and {excludelist_path:?}")
            })?;
        debug!(
            "Loaded {} include and {} exclude patterns",
            matcher.includes.len(),
            matcher.excludes.len()
        );
        Ok(matcher)
    }

    pub fn from_lines(includes: &[&str], excludes: &[&str]) -> Result<Self, anyhow::Error> {
        Ok(Self {
            includes: includes
                .iter()
                .map(|p| CompiledPattern::compile(p))
                .collect::<Result<Vec<_>, _>>()?,
            excludes: excludes
                .iter()
                .map(|p| CompiledPattern::compile(p))
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    /// The include pattern matching `path`, if any.
    ///
    /// `path` is normalised internally; callers pass a raw path string.
    pub fn matched_include(&self, path: &str) -> Option<&str> {
        Self::first_match(&self.includes, path)
    }

    /// The exclude pattern matching `path`, if any.
    pub fn matched_exclude(&self, path: &str) -> Option<&str> {
        Self::first_match(&self.excludes, path)
    }

    /// Every include pattern, for "was not required" reporting.
    pub fn include_patterns(&self) -> impl Iterator<Item = &str> + '_ {
        self.includes.iter().map(|c| c.pattern.as_str())
    }

    fn first_match<'a>(patterns: &'a [CompiledPattern], path: &str) -> Option<&'a str> {
        let normalized = normalize_path_for_matching(path);
        patterns
            .iter()
            .find(|c| c.regex.is_match(&normalized))
            .map(|c| c.pattern.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matcher(includes: &[&str], excludes: &[&str]) -> ListMatcher {
        ListMatcher::from_lines(includes, excludes).expect("patterns must compile")
    }

    /// The single most important test in this module.
    ///
    /// List entries are path patterns. A bare library name has no `/` and so
    /// cannot match `*/name.dll`. If this inverts, every excludelist prune
    /// becomes a fatal "not in include list" error (loud), or -- worse, if the
    /// synthetic prefix is ever dropped from the include side -- a real
    /// dependency is silently reclassified.
    #[test]
    fn bare_name_needs_the_synthetic_directory_prefix() {
        let m = matcher(&[], &["*/kernel32.dll"]);

        assert_eq!(m.matched_exclude("kernel32.dll"), None);

        let synthetic = synthetic_unresolved_path("kernel32.dll");
        assert_eq!(
            m.matched_exclude(&synthetic.to_string_lossy()),
            Some("*/kernel32.dll")
        );
    }

    #[test]
    fn synthetic_prefix_contains_no_regex_metacharacters() {
        // If UNRESOLVED_DIR ever gained a character that survived translation
        // as a metacharacter, matching would break in confusing ways.
        let m = matcher(&[&format!("{UNRESOLVED_DIR}/x.dll")], &[]);
        assert_eq!(
            m.matched_include(&synthetic_unresolved_path("x.dll").to_string_lossy()),
            Some(format!("{UNRESOLVED_DIR}/x.dll").as_str())
        );
    }

    #[test]
    fn matching_is_case_insensitive_in_both_directions() {
        // The package really does ship `libmp3lame.DLL` with an uppercase
        // extension, and import table entries are mixed-case.
        let m = matcher(&["*/libmp3lame.DLL", "*/user32.dll"], &[]);
        assert_eq!(
            m.matched_include("c:/vcpkg/bin/libmp3lame.dll"),
            Some("*/libmp3lame.DLL")
        );
        assert_eq!(
            m.matched_include("C:/VCPKG/BIN/LIBMP3LAME.DLL"),
            Some("*/libmp3lame.DLL")
        );
        assert_eq!(
            m.matched_include("C:/Windows/System32/USER32.dll"),
            Some("*/user32.dll")
        );
    }

    #[test]
    fn backslash_paths_are_normalized() {
        let m = matcher(&["*/Qt6*.dll"], &[]);
        assert_eq!(
            m.matched_include("C:\\vcpkg\\installed\\x64-windows\\bin\\Qt6QuickControls2.dll"),
            Some("*/Qt6*.dll")
        );
    }

    #[test]
    fn star_stays_a_wildcard_and_dot_is_literal() {
        let m = matcher(&["*/icu*.dll", "*/pcre2-*.dll", "*/libcrypto-*.dll"], &[]);
        assert_eq!(m.matched_include("/x/icudt74.dll"), Some("*/icu*.dll"));
        assert_eq!(m.matched_include("/x/pcre2-16.dll"), Some("*/pcre2-*.dll"));
        assert_eq!(
            m.matched_include("/x/libcrypto-3-x64.dll"),
            Some("*/libcrypto-*.dll")
        );

        // A literal dot must not behave as `.`: `*/icu*.dll` should not match a
        // name where the dot position is occupied by another character.
        let m = matcher(&["*/abc.dll"], &[]);
        assert_eq!(m.matched_include("/x/abcXdll"), None);
    }

    /// `*/d3d*.dll` already covers d3d9/11/12 but NOT d2d1.dll, which is why
    /// the excludelist needs its own entry for Direct2D.
    #[test]
    fn d3d_pattern_does_not_cover_d2d1() {
        let m = matcher(&[], &["*/d3d*.dll"]);
        assert_eq!(
            m.matched_exclude("C:/Windows/System32/d3d12.dll"),
            Some("*/d3d*.dll")
        );
        assert_eq!(m.matched_exclude("C:/Windows/System32/d2d1.dll"), None);
    }

    #[test]
    fn plus_is_escaped_not_treated_as_a_quantifier() {
        let m = matcher(&["*/libstdc++*.dll"], &[]);
        assert_eq!(
            m.matched_include("/x/libstdc++-6.dll"),
            Some("*/libstdc++*.dll")
        );
        // Were `+` a quantifier, `libstdc` followed by any number of `c` would
        // match and this would wrongly succeed.
        assert_eq!(m.matched_include("/x/libstdc-6.dll"), None);
    }

    #[test]
    fn blank_lines_and_comments_are_skipped() {
        // A blank line would compile to the empty regex, which matches
        // everything -- an excludelist ending in a stray newline would have
        // excluded every dependency.
        let parsed = parse_list("*/a.dll\n\n  \n# a comment\n*/b.dll\n");
        assert_eq!(parsed, vec!["*/a.dll", "*/b.dll"]);

        let m = matcher(&[], &parsed);
        assert_eq!(m.matched_exclude("/x/totally-unrelated.dll"), None);
    }

    #[test]
    fn reports_the_matching_pattern_not_the_filename() {
        // The `used_includes` report compares against patterns, so
        // matched_include must return the pattern.
        let m = matcher(&["*/Qt6*.dll"], &[]);
        assert_eq!(m.matched_include("/x/Qt6Core.dll"), Some("*/Qt6*.dll"));
        assert_eq!(
            m.include_patterns().collect::<Vec<_>>(),
            vec!["*/Qt6*.dll"]
        );
    }

    #[test]
    fn a_path_can_match_both_lists() {
        // Precedence (include wins) is the caller's decision, not the
        // matcher's; the matcher just reports both.
        let m = matcher(&["*/dbghelp.dll"], &["*/dbghelp.dll"]);
        let p = "C:/Windows/System32/dbghelp.dll";
        assert!(m.matched_include(p).is_some());
        assert!(m.matched_exclude(p).is_some());
    }

    #[test]
    fn invalid_pattern_is_rejected_at_load_time() {
        // `[` survives translation and is an unterminated character class.
        let err = match ListMatcher::from_lines(&["*/["], &[]) {
            Ok(_) => panic!("an unterminated character class must not compile"),
            Err(e) => e,
        };
        let msg = format!("{err:#}");
        assert!(msg.contains("*/["), "error should name the pattern: {msg}");
    }

    /// Regression guard: every pattern actually committed to the repo must
    /// compile. Without this, a typo in a list file fails a packaging run
    /// rather than a test.
    #[test]
    fn committed_lists_all_compile() {
        const LISTS: &[(&str, &str)] = &[
            (
                "windows/includelist",
                include_str!("../../../../distribution/windows/includelist"),
            ),
            (
                "windows/excludelist",
                include_str!("../../../../distribution/windows/excludelist"),
            ),
            (
                "macos/includelist",
                include_str!("../../../../distribution/macos/includelist"),
            ),
            (
                "macos/excludelist",
                include_str!("../../../../distribution/macos/excludelist"),
            ),
            (
                "linux/includelist",
                include_str!("../../../../distribution/linux/includelist"),
            ),
            (
                "linux/excludelist",
                include_str!("../../../../distribution/linux/excludelist"),
            ),
        ];

        for (name, body) in LISTS {
            let patterns = parse_list(body);
            ListMatcher::from_lines(&patterns, &[])
                .unwrap_or_else(|e| panic!("{name} failed to compile: {e:#}"));
        }
    }
}
