//! Platform-agnostic dependency closure walker.
//!
//! # Why this exists
//!
//! The packaging step used to determine dependencies by running a per-platform
//! subprocess (`Dependencies.exe` on Windows, `otool` on macOS) against a
//! **single** root: the application executable. Qt's QML plugins are loaded at
//! runtime via `LoadLibrary`/`dlopen`, so nothing statically imports them and
//! they are unreachable from the executable at any depth. Their own
//! dependencies -- `Qt6QuickControls2`, `Qt6QuickTemplates2`, and 46 more --
//! were therefore never bundled, and the shipped package could not load its UI.
//!
//! The fix is to seed the walk with **every binary already staged in the output
//! folder**, which is what the Linux path has always done (see
//! `scripts/linux_deps.sh`, which `patchelf --add-needed`s every `*.so*` in the
//! folder onto a temporary copy of the executable before running `lddtree`).
//!
//! # Shape
//!
//! Only two operations are platform-specific, and they are the whole of
//! [`BinaryScanner`]: reading the library references a binary declares, and
//! resolving one of those references to a file. Everything else -- root
//! enumeration, the breadth-first traversal, deduplication, classification
//! order, and tree construction -- lives here and is shared.

use anyhow::Context;
use indexmap::IndexMap;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::list_matcher::{synthetic_unresolved_path, ListMatcher};

use common::logging::macros::*;
shoop_log_unit!("packaging");

/// A dependency tree as consumed by `dependencies::collect_deps`.
///
/// `children_indent` and `maybe_parent` exist only to serve the indentation
/// parser used by the Linux subprocess path; the walker leaves them at their
/// defaults.
#[derive(Default)]
pub struct InternalDependency {
    pub path: PathBuf,
    pub deps: IndexMap<PathBuf, Rc<RefCell<InternalDependency>>>,
    pub children_indent: usize,
    pub maybe_parent: Option<Rc<RefCell<InternalDependency>>>,
}

/// One library reference exactly as recorded inside a binary.
///
/// Verbatim and unresolved: `"Qt6Core.dll"`, `"@rpath/libQt6Core.6.dylib"`,
/// `"/usr/lib/libSystem.B.dylib"`, `"@loader_path/../../lib/libfoo.dylib"`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LibraryReference {
    pub raw: String,
}

impl LibraryReference {
    pub fn new(raw: impl Into<String>) -> Self {
        Self { raw: raw.into() }
    }
}

/// What a scanner learned by reading one binary.
pub struct ScannedBinary<C> {
    pub references: Vec<LibraryReference>,
    /// Everything needed to resolve `references`, and nothing else. `()` on
    /// platforms whose resolution rules do not depend on the referencing
    /// binary.
    pub context: C,
}

/// The outcome of resolving one reference.
///
/// Failure to resolve is *data*, not an error: the driver, not the scanner,
/// decides what a failure means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    /// Present on disk. The only outcome that can be bundled.
    Found(PathBuf),
    /// A well-formed path that is not present on disk. Distinguished from
    /// [`Resolution::Unresolvable`] only by having a better diagnostic.
    Nominal(PathBuf),
    /// Could not be turned into any path that exists. Always a packaging bug.
    Unresolvable { tried: Vec<PathBuf> },
}

/// Invariants for the whole walk.
pub struct ResolveEnv {
    /// Explicit search directories, highest priority first.
    ///
    /// This replaces the previous approach of mutating `PATH` /
    /// `DYLD_LIBRARY_PATH` via `common::env::add_lib_search_path` and letting a
    /// subprocess resolve against it, which made the bundled set depend on the
    /// build machine's environment.
    pub search_dirs: Vec<PathBuf>,
    /// Directory of the main executable (macOS `@executable_path`).
    pub executable_dir: PathBuf,
    /// The output folder being populated.
    pub output_dir: PathBuf,
}

/// The two platform-specific operations.
pub trait BinaryScanner {
    /// Per-binary resolution context. Use `()` where resolution does not depend
    /// on the referencing binary.
    type Context: Clone;

    /// Human-readable object format name, for log messages.
    fn format_name(&self) -> &'static str;

    /// Cheap test used when enumerating roots. Must not parse the whole file.
    fn is_binary(&self, path: &Path) -> bool;

    /// Read `binary`.
    ///
    /// `Ok(None)` means "not a loadable image of this format" and the driver
    /// skips it silently. `Err` means the file looked like one of ours and was
    /// malformed.
    fn scan(&self, binary: &Path) -> Result<Option<ScannedBinary<Self::Context>>, anyhow::Error>;

    /// Stable key for a raw reference: the identity under which a dependency is
    /// classified exactly once. Lowercased file name, with any platform prefix
    /// (`@rpath/` and friends) stripped.
    fn reference_key(&self, reference: &LibraryReference) -> String;

    /// The same key, derived from a concrete file path.
    fn path_key(&self, path: &Path) -> String;

    /// Whether the operating system supplies this reference, and why.
    ///
    /// Purely name- or prefix-based: **must not touch the filesystem**. On
    /// Windows this recognises API-set names, which have no file anywhere. On
    /// macOS it recognises the dyld-shared-cache path prefixes, whose libraries
    /// exist as files on some OS versions and not others -- so existence is the
    /// wrong question to ask, and asking it first would resolve and bundle
    /// something the OS already provides.
    fn provided_reason(&self, reference: &LibraryReference) -> Option<String>;

    /// Resolve one reference.
    ///
    /// `chain[0]` is the context of the binary declaring `reference`;
    /// `chain[1..]` are the contexts of the binaries that load it, nearest
    /// first, ending with the main executable. This mirrors dyld's run-path
    /// accumulation. Context-free implementations ignore `chain` entirely.
    fn resolve(
        &self,
        reference: &LibraryReference,
        chain: &[Self::Context],
        env: &ResolveEnv,
    ) -> Resolution;
}

/// A dependency selected for bundling.
#[derive(Clone, Debug)]
pub struct ToCopy {
    pub path: PathBuf,
    /// The include pattern that selected it.
    pub matched_pattern: String,
}

/// A dependency that could not be resolved to a file.
#[derive(Clone, Debug, Default)]
pub struct Missing {
    /// The include pattern that matched, if any.
    pub matched_pattern: Option<String>,
    /// Candidate paths that were tried, when known.
    pub tried: Vec<PathBuf>,
    /// A nominal path, when the reference named one but no file was there.
    pub nominal: Option<PathBuf>,
}

/// Everything the walk observed. Ordered maps so reports are deterministic.
#[derive(Default)]
pub struct ScanReport {
    pub seeds: Vec<PathBuf>,
    /// Already staged in the output folder: traversed, not copied.
    pub in_folder: BTreeMap<String, PathBuf>,
    /// Matched the includelist and resolved: these get copied.
    pub to_copy: BTreeMap<String, ToCopy>,
    /// Matched the excludelist. Pruned by name, never resolved.
    pub excluded: BTreeMap<String, String>,
    /// Provided by the operating system. Pruned, never an error.
    pub provided: BTreeMap<String, String>,
    /// In the includelist but no file found anywhere. An error.
    pub unresolved: BTreeMap<String, Missing>,
    /// Matched neither list. An error, and the excludelist bootstrap candidate
    /// set.
    pub unclassified: BTreeMap<String, Missing>,
    /// Every binary that referenced each key, by file name.
    pub importers: BTreeMap<String, BTreeSet<String>>,
    /// Files with a plausible extension that did not parse as this format.
    pub skipped_non_binaries: Vec<PathBuf>,
    pub edges_walked: usize,
}

impl ScanReport {
    /// Keys that represent a packaging failure.
    pub fn problem_count(&self) -> usize {
        self.unresolved.len() + self.unclassified.len()
    }

    /// Importers of `key`, as a comma-separated list capped for readability.
    pub fn importers_summary(&self, key: &str) -> String {
        let Some(importers) = self.importers.get(key) else {
            return String::from("(none recorded)");
        };
        let shown: Vec<&str> = importers.iter().take(4).map(String::as_str).collect();
        if importers.len() > shown.len() {
            format!(
                "{} (+{} more)",
                shown.join(", "),
                importers.len() - shown.len()
            )
        } else {
            shown.join(", ")
        }
    }
}

/// Index of the binaries already staged in the output folder.
///
/// A reference resolving here means "already in the folder": the walk descends
/// into it, but it is not copied again.
pub struct FolderIndex {
    by_key: HashMap<String, PathBuf>,
}

impl FolderIndex {
    pub fn build<S: BinaryScanner>(folder: &Path, scanner: &S) -> Result<Self, anyhow::Error> {
        let mut by_key: HashMap<String, PathBuf> = HashMap::new();
        for path in walk_binaries(folder, scanner)? {
            let key = scanner.path_key(&path);
            if let Some(existing) = by_key.get(&key) {
                // Benign: both copies would be bundled anyway. Warn rather than
                // fail so a duplicate basename cannot block a release.
                warn!(
                    "  Duplicate binary name in output folder: {:?} and {:?}; keeping the first",
                    existing, path
                );
                continue;
            }
            by_key.insert(key, path);
        }
        Ok(Self { by_key })
    }

    pub fn get(&self, key: &str) -> Option<&Path> {
        self.by_key.get(key).map(PathBuf::as_path)
    }

    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }

    /// Paths in the index, sorted for deterministic seeding.
    pub fn paths(&self) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = self.by_key.values().cloned().collect();
        paths.sort();
        paths
    }
}

/// Every binary under `folder`, as judged by [`BinaryScanner::is_binary`].
///
/// Does not follow symlinks: on macOS the versioned-dylib symlinks in `lib/`
/// would otherwise be scanned twice under different names. Skips `*.dSYM`
/// payloads, which are parseable Mach-O images but not loadable ones -- seeding
/// one would make `@loader_path` resolve against the dSYM directory and produce
/// spurious errors.
fn walk_binaries<S: BinaryScanner>(
    folder: &Path,
    scanner: &S,
) -> Result<Vec<PathBuf>, anyhow::Error> {
    let mut found: Vec<PathBuf> = Vec::new();
    for entry in walkdir::WalkDir::new(folder).follow_links(false) {
        let entry = entry.with_context(|| format!("Cannot walk {folder:?}"))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path
            .components()
            .any(|c| c.as_os_str().to_string_lossy().ends_with(".dSYM"))
        {
            continue;
        }
        if scanner.is_binary(path) {
            found.push(path.to_path_buf());
        }
    }
    found.sort();
    Ok(found)
}

/// What to walk, and how strictly.
pub struct ScanRequest {
    /// Where the walk starts. Normally every binary in the output folder, with
    /// the main executable first.
    pub roots: Vec<PathBuf>,
    /// The output folder. Anything resolving inside it is "already in folder".
    pub output_folder: PathBuf,
    /// Collect problems into the report instead of failing. Used to bootstrap
    /// the include/exclude lists.
    pub report_only: bool,
    /// Stop descending at this depth. Only used to reproduce the old
    /// single-root, depth-capped traversal for before/after comparison.
    pub max_depth: Option<usize>,
}

/// Walk the dependency closure and produce a tree plus a report.
///
/// # Tree shape
///
/// The returned tree is **flat**: a synthetic root with one depth-1 child per
/// classified dependency, and no grandchildren. This is deliberate and
/// load-bearing for two reasons.
///
/// First, `collect_deps` recurses into a node's `deps` unconditionally, outside
/// its `handled` guard. Sharing nodes between parents -- the natural way to
/// represent a dependency *graph* -- would therefore re-descend already-handled
/// subtrees and recurse forever on the reference cycles that are routine among
/// dylibs.
///
/// Second, it makes the consumer order-independent. In a nested tree, a node
/// attached under a parent that is later pruned is silently dropped; that is
/// precisely the bug that the old Windows script's order-dependent
/// deduplication had, and one of the two mechanisms that hid the missing Qt
/// libraries. Here every node is reached directly from the root exactly once.
///
/// Because the tree carries no decision-relevant structure, this function must
/// classify every edge itself. `collect_deps` then re-derives the copy decision
/// from the same lists, as a cross-check, and remains the single source of
/// truth for what is copied.
#[tracing::instrument(name = "tool.packaging.dependency_tree", skip_all)]
pub fn build_dependency_tree<S: BinaryScanner>(
    scanner: &S,
    request: &ScanRequest,
    env: &ResolveEnv,
    matcher: &ListMatcher,
    folder_index: &FolderIndex,
    error_msgs: &mut String,
) -> Result<(Rc<RefCell<InternalDependency>>, ScanReport), anyhow::Error> {
    let mut report = ScanReport::default();
    report.seeds = request.roots.clone();

    // Classified exactly once per reference key. Distinct from `expanded`:
    // two different names can resolve to the same file (versioned dylib
    // symlinks), and both names must be bundled even though the file is only
    // worth parsing once.
    let mut classified: HashSet<String> = HashSet::new();
    // Expanded exactly once per file.
    let mut expanded: HashSet<PathBuf> = HashSet::new();

    // Each queue entry carries the references already read from the binary, so
    // every file is parsed exactly once for the whole walk.
    type QueueEntry<C> = (PathBuf, Vec<LibraryReference>, Vec<C>, usize);
    let mut queue: VecDeque<QueueEntry<S::Context>> = VecDeque::new();

    // The main executable's context applies when resolving every other binary's
    // references, so it is scanned first and kept as the tail of every chain.
    let main_scan = match request.roots.first() {
        Some(main) => scan_one(scanner, main, &mut report, error_msgs)?,
        None => None,
    };
    let main_context: Option<S::Context> = main_scan.as_ref().map(|s| s.context.clone());

    for (index, root) in request.roots.iter().enumerate() {
        // Deliberately not marked as classified here. Roots are reached again as
        // dependencies of each other, and letting them fall through to the
        // folder-index check is what records them as "already in folder" --
        // which is the report's evidence of what the package already satisfies.
        // Re-expansion is prevented by `expanded`, not by `classified`.
        if !expanded.insert(canonical_key(root)) {
            continue;
        }
        let scanned = if index == 0 {
            // Reuse the scan above rather than reading the executable twice.
            match &main_scan {
                Some(scanned) => ScannedBinary {
                    references: scanned.references.clone(),
                    context: scanned.context.clone(),
                },
                None => continue,
            }
        } else {
            match scan_one(scanner, root, &mut report, error_msgs)? {
                Some(scanned) => scanned,
                None => continue,
            }
        };
        let mut chain = vec![scanned.context];
        if index > 0 {
            if let Some(main) = main_context.clone() {
                chain.push(main);
            }
        }
        queue.push_back((root.clone(), scanned.references, chain, 0));
    }

    while let Some((path, references, chain, depth)) = queue.pop_front() {
        let importer_name = file_name_of(&path);

        for reference in &references {
            report.edges_walked += 1;
            let key = scanner.reference_key(reference);
            if key.is_empty() {
                continue;
            }
            report
                .importers
                .entry(key.clone())
                .or_default()
                .insert(importer_name.clone());

            // 1. Operating-system-provided. Must come before any filesystem
            //    lookup: System32 contains real files for some API-set names on
            //    some Windows SKUs, so resolving first would bundle them.
            if let Some(reason) = scanner.provided_reason(reference) {
                if classified.insert(key.clone()) {
                    debug!("  OS-provided, pruned: {} ({})", reference.raw, reason);
                    report.provided.insert(key, reason);
                }
                continue;
            }

            // 2. Already classified under this key.
            if classified.contains(&key) {
                continue;
            }

            // 3. Already staged in the output folder: traverse, don't copy.
            //    Checked before either list, matching `collect_deps`.
            if let Some(in_folder) = folder_index.get(&key) {
                classified.insert(key.clone());
                let in_folder = in_folder.to_path_buf();
                report.in_folder.insert(key, in_folder.clone());
                enqueue(
                    scanner,
                    &in_folder,
                    &chain,
                    depth,
                    request,
                    &mut expanded,
                    &mut queue,
                    &mut report,
                    error_msgs,
                )?;
                continue;
            }

            let synthetic = synthetic_unresolved_path(&key);
            let synthetic_str = synthetic.to_string_lossy().into_owned();
            let include_by_name = matcher.matched_include(&synthetic_str);
            let exclude_by_name = matcher.matched_exclude(&synthetic_str);

            // 4. Excluded by name: prune WITHOUT resolving.
            //
            //    This has to happen here rather than in the consumer. Resolving
            //    first and pruning later means walking into the operating
            //    system: hundreds of system libraries, and unresolvable
            //    references from deep inside them that could never be fixed.
            if exclude_by_name.is_some() && include_by_name.is_none() {
                classified.insert(key.clone());
                report
                    .excluded
                    .insert(key, exclude_by_name.unwrap().to_string());
                continue;
            }

            // 5. Resolve, then classify.
            classified.insert(key.clone());
            match scanner.resolve(reference, &chain, env) {
                Resolution::Found(resolved) => {
                    let matched = matcher.matched_include(&resolved.to_string_lossy());
                    match matched.or(include_by_name) {
                        Some(pattern) => {
                            report.to_copy.insert(
                                key,
                                ToCopy {
                                    path: resolved.clone(),
                                    matched_pattern: pattern.to_string(),
                                },
                            );
                            enqueue(
                                scanner,
                                &resolved,
                                &chain,
                                depth,
                                request,
                                &mut expanded,
                                &mut queue,
                                &mut report,
                                error_msgs,
                            )?;
                        }
                        None => {
                            record_problem(
                                &mut report.unclassified,
                                key,
                                Missing {
                                    matched_pattern: None,
                                    nominal: Some(resolved),
                                    tried: Vec::new(),
                                },
                            );
                        }
                    }
                }
                Resolution::Nominal(nominal) => {
                    let missing = Missing {
                        matched_pattern: include_by_name.map(str::to_string),
                        nominal: Some(nominal),
                        tried: Vec::new(),
                    };
                    let bucket = if include_by_name.is_some() {
                        &mut report.unresolved
                    } else {
                        &mut report.unclassified
                    };
                    record_problem(bucket, key, missing);
                }
                Resolution::Unresolvable { tried } => {
                    let missing = Missing {
                        matched_pattern: include_by_name.map(str::to_string),
                        nominal: None,
                        tried,
                    };
                    let bucket = if include_by_name.is_some() {
                        &mut report.unresolved
                    } else {
                        &mut report.unclassified
                    };
                    record_problem(bucket, key, missing);
                }
            }
        }
    }

    if !request.report_only {
        append_problem_errors(&report, error_msgs);
    }

    Ok((flat_tree(&report), report))
}

fn record_problem(bucket: &mut BTreeMap<String, Missing>, key: String, missing: Missing) {
    bucket.insert(key, missing);
}

/// Turn the report's problems into the error text the caller surfaces.
///
/// Naming both the missing library and the binaries that need it is the
/// diagnostic the old scanners lacked: `Dependencies.exe` reported a bare
/// "NotFound" line, and the macOS script dropped the dependency with a warning
/// nobody read.
fn append_problem_errors(report: &ScanReport, error_msgs: &mut String) {
    for (key, missing) in &report.unresolved {
        let pattern = missing.matched_pattern.as_deref().unwrap_or("?");
        error_msgs.push_str(&format!(
            "{key}: matches include pattern {pattern} but no file was found. Needed by: {}\n",
            report.importers_summary(key)
        ));
        if !missing.tried.is_empty() {
            error_msgs.push_str(&format!("    tried: {:?}\n", missing.tried));
        }
    }
    for (key, missing) in &report.unclassified {
        let where_ = match &missing.nominal {
            Some(p) => format!("resolved to {}", p.display()),
            None => String::from("not found"),
        };
        error_msgs.push_str(&format!(
            "{key}: is not in include list ({where_}). Needed by: {}\n",
            report.importers_summary(key)
        ));
    }
}

/// Build the flat depth-1 tree described on [`build_dependency_tree`].
///
/// Only classifications that are not errors become nodes. In particular,
/// unresolved dependencies are deliberately absent: emitting them would let
/// them match the includelist in the consumer, get selected for copying, and
/// then be silently skipped by the copy loop's "nonexistent file" branch --
/// reintroducing exactly the class of silent hole this change removes.
fn flat_tree(report: &ScanReport) -> Rc<RefCell<InternalDependency>> {
    let root: Rc<RefCell<InternalDependency>> =
        Rc::new(RefCell::new(InternalDependency::default()));

    let add = |path: PathBuf| {
        let node = Rc::new(RefCell::new(InternalDependency {
            path: path.clone(),
            ..Default::default()
        }));
        root.borrow_mut().deps.insert(path, node);
    };

    for path in report.in_folder.values() {
        add(path.clone());
    }
    for entry in report.to_copy.values() {
        add(entry.path.clone());
    }
    for key in report.excluded.keys() {
        add(synthetic_unresolved_path(key));
    }

    root
}

#[allow(clippy::too_many_arguments)]
fn enqueue<S: BinaryScanner>(
    scanner: &S,
    path: &Path,
    chain: &[S::Context],
    depth: usize,
    request: &ScanRequest,
    expanded: &mut HashSet<PathBuf>,
    queue: &mut VecDeque<(PathBuf, Vec<LibraryReference>, Vec<S::Context>, usize)>,
    report: &mut ScanReport,
    error_msgs: &mut String,
) -> Result<(), anyhow::Error> {
    if let Some(max) = request.max_depth {
        if depth + 1 >= max {
            return Ok(());
        }
    }
    if !expanded.insert(canonical_key(path)) {
        return Ok(());
    }
    let Some(scanned) = scan_one(scanner, path, report, error_msgs)? else {
        return Ok(());
    };
    // dyld accumulates run-paths from the loaded image up towards the main
    // executable, so the newly reached binary's own context goes in front.
    let mut child_chain = Vec::with_capacity(chain.len() + 1);
    child_chain.push(scanned.context);
    child_chain.extend(chain.iter().cloned());
    child_chain.truncate(MAX_CHAIN);
    queue.push_back((
        path.to_path_buf(),
        scanned.references,
        child_chain,
        depth + 1,
    ));
    Ok(())
}

/// Bound on the retained loader chain. Deep chains add no resolution power
/// because the global search directories are consulted as a fallback anyway.
const MAX_CHAIN: usize = 32;

/// Scan a binary, recording a malformed file as an error rather than aborting
/// the walk, so one bad file does not hide every other problem.
fn scan_one<S: BinaryScanner>(
    scanner: &S,
    path: &Path,
    report: &mut ScanReport,
    error_msgs: &mut String,
) -> Result<Option<ScannedBinary<S::Context>>, anyhow::Error> {
    match scanner.scan(path) {
        Ok(Some(scanned)) => Ok(Some(scanned)),
        Ok(None) => {
            debug!(
                "  Not a {} image, skipping: {:?}",
                scanner.format_name(),
                path
            );
            report.skipped_non_binaries.push(path.to_path_buf());
            Ok(None)
        }
        Err(e) => {
            error_msgs.push_str(&format!("{}: cannot read: {e:#}\n", path.display()));
            Ok(None)
        }
    }
}

/// Key for the "already expanded this file" set.
///
/// Canonicalizing collapses versioned-dylib symlink chains
/// (`libQt6Core.6.dylib` -> `libQt6Core.6.9.1.dylib`) onto one entry. The key is
/// internal and never printed or matched against a list, so platform-specific
/// canonical forms are harmless.
fn canonical_key(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An in-memory dependency graph. Exercises the shared driver without any
    /// filesystem or object-format involvement, so it runs identically on every
    /// platform.
    struct FakeScanner {
        /// file name -> references it declares
        graph: HashMap<String, Vec<String>>,
        /// names that resolve, and to where
        resolvable: HashMap<String, PathBuf>,
        /// names classified as OS-provided
        provided: HashSet<String>,
    }

    impl FakeScanner {
        fn new() -> Self {
            Self {
                graph: HashMap::new(),
                resolvable: HashMap::new(),
                provided: HashSet::new(),
            }
        }

        /// Declare that `from` references `to`, and that each `to` resolves to
        /// a synthetic external location unless already known.
        fn edge(mut self, from: &str, to: &[&str]) -> Self {
            self.graph.insert(
                from.to_lowercase(),
                to.iter().map(|s| s.to_string()).collect(),
            );
            for t in to {
                self.resolvable
                    .entry(t.to_lowercase())
                    .or_insert_with(|| PathBuf::from("/ext").join(t));
            }
            self
        }

        fn unresolvable(mut self, name: &str) -> Self {
            self.resolvable.remove(&name.to_lowercase());
            self
        }

        fn provided(mut self, name: &str) -> Self {
            self.provided.insert(name.to_lowercase());
            self
        }
    }

    impl BinaryScanner for FakeScanner {
        type Context = String;

        fn format_name(&self) -> &'static str {
            "Fake"
        }

        fn is_binary(&self, _path: &Path) -> bool {
            true
        }

        fn scan(
            &self,
            binary: &Path,
        ) -> Result<Option<ScannedBinary<Self::Context>>, anyhow::Error> {
            let key = self.path_key(binary);
            let references = self
                .graph
                .get(&key)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(LibraryReference::new)
                .collect();
            Ok(Some(ScannedBinary {
                references,
                context: key,
            }))
        }

        fn reference_key(&self, reference: &LibraryReference) -> String {
            reference.raw.to_lowercase()
        }

        fn path_key(&self, path: &Path) -> String {
            path.file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default()
        }

        fn provided_reason(&self, reference: &LibraryReference) -> Option<String> {
            self.provided
                .contains(&self.reference_key(reference))
                .then(|| String::from("fake system library"))
        }

        fn resolve(
            &self,
            reference: &LibraryReference,
            _chain: &[Self::Context],
            _env: &ResolveEnv,
        ) -> Resolution {
            let key = self.reference_key(reference);
            match self.resolvable.get(&key) {
                Some(p) => Resolution::Found(p.clone()),
                None => Resolution::Unresolvable {
                    tried: vec![PathBuf::from("/ext").join(&key)],
                },
            }
        }
    }

    struct Harness {
        report: ScanReport,
        errors: String,
        tree_paths: Vec<PathBuf>,
    }

    fn run(
        scanner: &FakeScanner,
        roots: &[&str],
        includes: &[&str],
        excludes: &[&str],
        report_only: bool,
    ) -> Harness {
        run_with_folder(scanner, roots, &[], includes, excludes, report_only)
    }

    /// `folder_names` are treated as already staged in the output folder.
    fn run_with_folder(
        scanner: &FakeScanner,
        roots: &[&str],
        folder_names: &[&str],
        includes: &[&str],
        excludes: &[&str],
        report_only: bool,
    ) -> Harness {
        let matcher = ListMatcher::from_lines(includes, excludes).expect("patterns compile");
        let mut by_key = HashMap::new();
        for name in folder_names {
            by_key.insert(name.to_lowercase(), PathBuf::from("/pkg").join(name));
        }
        let folder_index = FolderIndex { by_key };
        let request = ScanRequest {
            roots: roots
                .iter()
                .map(|r| PathBuf::from("/pkg").join(r))
                .collect(),
            output_folder: PathBuf::from("/pkg"),
            report_only,
            max_depth: None,
        };
        let env = ResolveEnv {
            search_dirs: vec![PathBuf::from("/ext")],
            executable_dir: PathBuf::from("/pkg"),
            output_dir: PathBuf::from("/pkg"),
        };
        let mut errors = String::new();
        let (tree, report) = build_dependency_tree(
            scanner,
            &request,
            &env,
            &matcher,
            &folder_index,
            &mut errors,
        )
        .expect("walk must not fail");
        let tree_paths = tree.borrow().deps.keys().cloned().collect();
        Harness {
            report,
            errors,
            tree_paths,
        }
    }

    /// The actual bug: a dependency reachable only from a plugin, never from
    /// the executable. Seeding from the whole folder finds it; seeding from the
    /// executable alone does not.
    #[test]
    fn dependency_reachable_only_from_a_plugin_is_found() {
        let scanner = FakeScanner::new()
            .edge("app.exe", &["Qt6Core.dll"])
            .edge("qtquickcontrols2plugin.dll", &["Qt6QuickControls2.dll"]);

        let all = run(
            &scanner,
            &["app.exe", "qtquickcontrols2plugin.dll"],
            &["*/Qt6*.dll"],
            &[],
            false,
        );
        assert!(all.report.to_copy.contains_key("qt6quickcontrols2.dll"));
        assert!(all.errors.is_empty(), "unexpected errors: {}", all.errors);
        assert_eq!(
            all.report.importers_summary("qt6quickcontrols2.dll"),
            "qtquickcontrols2plugin.dll"
        );

        let exe_only = run(&scanner, &["app.exe"], &["*/Qt6*.dll"], &[], false);
        assert!(
            !exe_only
                .report
                .to_copy
                .contains_key("qt6quickcontrols2.dll"),
            "single-root walk must not find it -- that is the bug"
        );
    }

    #[test]
    fn two_cycle_terminates() {
        let scanner = FakeScanner::new()
            .edge("a.dll", &["b.dll"])
            .edge("b.dll", &["a.dll"]);
        let h = run(&scanner, &["a.dll"], &["*/*.dll"], &[], false);
        assert!(h.report.to_copy.contains_key("b.dll"));
    }

    #[test]
    fn three_cycle_terminates() {
        let scanner = FakeScanner::new()
            .edge("a.dll", &["b.dll"])
            .edge("b.dll", &["c.dll"])
            .edge("c.dll", &["a.dll"]);
        let h = run(&scanner, &["a.dll"], &["*/*.dll"], &[], false);
        assert!(h.report.to_copy.contains_key("b.dll"));
        assert!(h.report.to_copy.contains_key("c.dll"));
    }

    #[test]
    fn diamond_classifies_once_and_records_both_importers() {
        let scanner = FakeScanner::new()
            .edge("root.dll", &["a.dll", "b.dll"])
            .edge("a.dll", &["shared.dll"])
            .edge("b.dll", &["shared.dll"]);
        let h = run(&scanner, &["root.dll"], &["*/*.dll"], &[], false);

        assert!(h.report.to_copy.contains_key("shared.dll"));
        let importers = &h.report.importers["shared.dll"];
        assert!(importers.contains("a.dll"), "got {importers:?}");
        assert!(importers.contains("b.dll"), "got {importers:?}");

        // Exactly one tree node for it, despite two edges.
        let count = h
            .tree_paths
            .iter()
            .filter(|p| p.file_name().unwrap() == "shared.dll")
            .count();
        assert_eq!(count, 1);
    }

    /// Pruning at excluded nodes has to happen in the producer. If it does not,
    /// the walk descends into the operating system.
    #[test]
    fn children_of_an_excluded_node_are_never_reached() {
        let scanner = FakeScanner::new()
            .edge("app.exe", &["kernel32.dll"])
            .edge("kernel32.dll", &["deep-system-thing.dll"]);
        let h = run(
            &scanner,
            &["app.exe"],
            &["*/app.exe"],
            &["*/kernel32.dll"],
            false,
        );

        assert!(h.report.excluded.contains_key("kernel32.dll"));
        assert!(!h.report.importers.contains_key("deep-system-thing.dll"));
        assert!(!h.report.to_copy.contains_key("deep-system-thing.dll"));
        assert!(!h.report.unclassified.contains_key("deep-system-thing.dll"));
    }

    #[test]
    fn already_in_folder_beats_the_excludelist() {
        let scanner = FakeScanner::new().edge("app.exe", &["staged.dll"]);
        let h = run_with_folder(
            &scanner,
            &["app.exe"],
            &["staged.dll"],
            &[],
            &["*/staged.dll"],
            false,
        );
        assert!(h.report.in_folder.contains_key("staged.dll"));
        assert!(!h.report.excluded.contains_key("staged.dll"));
    }

    #[test]
    fn includelist_beats_excludelist() {
        let scanner = FakeScanner::new().edge("app.exe", &["dbghelp.dll"]);
        let h = run(
            &scanner,
            &["app.exe"],
            &["*/dbghelp.dll"],
            &["*/dbghelp.dll"],
            false,
        );
        assert!(h.report.to_copy.contains_key("dbghelp.dll"));
        assert!(!h.report.excluded.contains_key("dbghelp.dll"));
    }

    #[test]
    fn os_provided_is_pruned_and_never_an_error() {
        let scanner = FakeScanner::new()
            .edge("app.exe", &["api-ms-win-crt-heap-l1-1-0.dll"])
            .unresolvable("api-ms-win-crt-heap-l1-1-0.dll")
            .provided("api-ms-win-crt-heap-l1-1-0.dll");
        let h = run(&scanner, &["app.exe"], &[], &[], false);

        assert!(h
            .report
            .provided
            .contains_key("api-ms-win-crt-heap-l1-1-0.dll"));
        assert!(h.errors.is_empty(), "unexpected errors: {}", h.errors);
        assert_eq!(h.report.problem_count(), 0);
    }

    /// The Tier-3 diagnostic: the error must name the missing library AND the
    /// binaries that need it.
    #[test]
    fn unresolved_error_names_the_library_and_its_importers() {
        let scanner = FakeScanner::new()
            .edge("qtquickcontrols2plugin.dll", &["Qt6QuickControls2.dll"])
            .unresolvable("Qt6QuickControls2.dll");
        let h = run(
            &scanner,
            &["qtquickcontrols2plugin.dll"],
            &["*/Qt6*.dll"],
            &[],
            false,
        );

        assert!(h.report.unresolved.contains_key("qt6quickcontrols2.dll"));
        assert!(
            h.errors.contains("qt6quickcontrols2.dll"),
            "errors: {}",
            h.errors
        );
        assert!(
            h.errors.contains("qtquickcontrols2plugin.dll"),
            "errors must name the importer: {}",
            h.errors
        );
    }

    #[test]
    fn unlisted_dependency_is_unclassified_not_copied() {
        let scanner = FakeScanner::new().edge("app.exe", &["mystery.dll"]);
        let h = run(&scanner, &["app.exe"], &[], &[], false);
        assert!(h.report.unclassified.contains_key("mystery.dll"));
        assert!(!h.report.to_copy.contains_key("mystery.dll"));
        assert!(h.errors.contains("is not in include list"));
    }

    #[test]
    fn report_only_suppresses_the_error_text() {
        let scanner = FakeScanner::new().edge("app.exe", &["mystery.dll"]);
        let strict = run(&scanner, &["app.exe"], &[], &[], false);
        assert!(!strict.errors.is_empty());

        let lenient = run(&scanner, &["app.exe"], &[], &[], true);
        assert!(lenient.errors.is_empty(), "errors: {}", lenient.errors);
        // The problem is still recorded, which is what makes bootstrapping the
        // lists possible.
        assert!(lenient.report.unclassified.contains_key("mystery.dll"));
    }

    /// Unresolved dependencies must not become tree nodes: the consumer would
    /// select them for copying and the copy loop would silently skip them.
    #[test]
    fn unresolved_dependencies_are_absent_from_the_tree() {
        let scanner = FakeScanner::new()
            .edge("app.exe", &["Qt6Missing.dll"])
            .unresolvable("Qt6Missing.dll");
        let h = run(&scanner, &["app.exe"], &["*/Qt6*.dll"], &[], true);

        assert!(h.report.unresolved.contains_key("qt6missing.dll"));
        assert!(
            !h.tree_paths
                .iter()
                .any(|p| p.to_string_lossy().contains("Qt6Missing")),
            "tree: {:?}",
            h.tree_paths
        );
    }

    /// Order independence: the flat tree means classification cannot depend on
    /// which importer happened to reach a dependency first.
    #[test]
    fn root_order_does_not_change_the_result() {
        let scanner = FakeScanner::new()
            .edge("a.dll", &["shared.dll"])
            .edge("b.dll", &["shared.dll"]);

        let forward = run(&scanner, &["a.dll", "b.dll"], &["*/*.dll"], &[], false);
        let reverse = run(&scanner, &["b.dll", "a.dll"], &["*/*.dll"], &[], false);

        assert_eq!(
            forward.report.to_copy.keys().collect::<Vec<_>>(),
            reverse.report.to_copy.keys().collect::<Vec<_>>()
        );
        assert_eq!(
            forward.report.importers["shared.dll"],
            reverse.report.importers["shared.dll"]
        );
    }

    #[test]
    fn max_depth_reproduces_a_shallow_traversal() {
        let scanner = FakeScanner::new()
            .edge("app.exe", &["l1.dll"])
            .edge("l1.dll", &["l2.dll"])
            .edge("l2.dll", &["l3.dll"]);

        let matcher = ListMatcher::from_lines(&["*/*.dll", "*/*.exe"], &[]).unwrap();
        let folder_index = FolderIndex {
            by_key: HashMap::new(),
        };
        let env = ResolveEnv {
            search_dirs: vec![PathBuf::from("/ext")],
            executable_dir: PathBuf::from("/pkg"),
            output_dir: PathBuf::from("/pkg"),
        };
        let mut errors = String::new();
        let (_tree, report) = build_dependency_tree(
            &scanner,
            &ScanRequest {
                roots: vec![PathBuf::from("/pkg/app.exe")],
                output_folder: PathBuf::from("/pkg"),
                report_only: true,
                max_depth: Some(1),
            },
            &env,
            &matcher,
            &folder_index,
            &mut errors,
        )
        .unwrap();

        assert!(report.to_copy.contains_key("l1.dll"));
        assert!(
            !report.to_copy.contains_key("l2.dll"),
            "depth cap must stop the descent"
        );
    }
}
