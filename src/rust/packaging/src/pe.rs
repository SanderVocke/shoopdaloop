//! Windows PE import reading and dependency resolution.
//!
//! Replaces the previous `Dependencies.exe`-plus-PowerShell scanner
//! (`scripts/windows_deps.ps1`). Beyond removing an external tool dependency and
//! a fragile indentation-scraping pipeline, doing this in process is what makes
//! it possible to seed the walk with all ~130 binaries in the package instead of
//! the one root `Dependencies.exe` accepts.

use anyhow::Context;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::deps_walker::{BinaryScanner, LibraryReference, Resolution, ResolveEnv, ScannedBinary};

use common::logging::macros::*;
shoop_log_unit!("packaging");

/// File extensions worth attempting to parse as PE.
///
/// The package holds ~2450 files of which only ~130 are PE and ~1141 are PNGs,
/// so filtering by extension before reading avoids a lot of pointless I/O.
const PE_EXTENSIONS: &[&str] = &["dll", "exe", "pyd", "ocx"];

/// Where a search directory came from. Only affects logging, but a dependency
/// silently starting to resolve out of `System32` is exactly the kind of drift
/// worth seeing in a build log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchDirKind {
    /// vcpkg / Qt runtime directories.
    Vcpkg,
    /// Operator-supplied via `SHOOP_EXTRA_DEP_SEARCH_DIRS`.
    Extra,
    /// `System32` and `%SystemRoot%`.
    System,
}

/// A search directory plus a case-insensitive index of its file names.
///
/// Indexing once beats `Path::exists()` per candidate: NTFS is
/// case-insensitive so `exists()` would also work, but an explicit lowercased
/// index behaves identically on every filesystem and avoids one stat per
/// directory per lookup.
struct IndexedDir {
    path: PathBuf,
    kind: SearchDirKind,
    by_name: HashMap<String, PathBuf>,
}

impl IndexedDir {
    fn build(path: &Path, kind: SearchDirKind) -> Self {
        let mut by_name: HashMap<String, PathBuf> = HashMap::new();
        match std::fs::read_dir(path) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let entry_path = entry.path();
                    if !entry_path.is_file() {
                        continue;
                    }
                    if let Some(name) = entry_path.file_name() {
                        by_name
                            .entry(name.to_string_lossy().to_lowercase())
                            .or_insert(entry_path);
                    }
                }
            }
            Err(e) => {
                debug!("  Search dir {:?} is not readable ({e}); skipping", path);
            }
        }
        Self {
            path: path.to_path_buf(),
            kind,
            by_name,
        }
    }
}

/// The library references a PE file declares.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PeImports {
    /// Normal import table entries.
    pub libraries: Vec<String>,
    /// Delay-load import table entries.
    pub delay_libraries: Vec<String>,
    pub machine: u16,
}

impl PeImports {
    /// All referenced names, normal imports first.
    pub fn all(&self) -> impl Iterator<Item = &String> {
        self.libraries.iter().chain(self.delay_libraries.iter())
    }
}

/// Whether `path` has an extension worth trying to parse.
pub fn has_pe_extension(path: &Path) -> bool {
    path.extension()
        .map(|e| {
            let e = e.to_string_lossy().to_lowercase();
            PE_EXTENSIONS.contains(&e.as_str())
        })
        .unwrap_or(false)
}

/// Read the import tables of a PE file.
///
/// `Ok(None)` means the file is not a PE image at all -- a `.dll`-named data
/// file, for instance. That is not an error; the caller skips it.
pub fn read_pe_imports(path: &Path) -> Result<Option<PeImports>, anyhow::Error> {
    let bytes = std::fs::read(path).with_context(|| format!("Cannot read {}", path.display()))?;
    Ok(parse_pe_imports(&bytes))
}

/// Parse import tables out of PE bytes. Pure, so it is unit-testable.
pub fn parse_pe_imports(bytes: &[u8]) -> Option<PeImports> {
    if bytes.len() < 0x40 || bytes.get(0..2) != Some(b"MZ") {
        return None;
    }
    let opts = goblin::pe::options::ParseOptions {
        resolve_rva: true,
        // Signed Qt and system DLLs carry certificate tables that are of no
        // interest here and cost time to parse.
        parse_attribute_certificates: false,
    };
    let pe = goblin::pe::PE::parse_with_opts(bytes, &opts).ok()?;

    let libraries: Vec<String> = pe.libraries.iter().map(|s| s.to_string()).collect();
    let delay_libraries = parse_delay_imports(bytes, &pe);

    Some(PeImports {
        libraries,
        delay_libraries,
        machine: pe.header.coff_header.machine,
    })
}

/// The machine type of a PE file, or `None` if it is not a PE file.
pub fn machine_of(path: &Path) -> Option<u16> {
    let bytes = std::fs::read(path).ok()?;
    parse_pe_imports(&bytes).map(|i| i.machine)
}

/// Size of `IMAGE_DELAYLOAD_DESCRIPTOR`.
const SIZEOF_DELAYLOAD_DESCRIPTOR: usize = 32;
/// `dlattrRva`: the descriptor's address fields are RVAs rather than absolute
/// virtual addresses. Every MSVC toolchain since VC6 sets this.
const DELAYLOAD_ATTR_RVA: u32 = 1;

/// Parse the delay-load import directory.
///
/// goblin does not expose data directory 13 in any version, so this walks it by
/// hand. It is best-effort: a malformed directory warns and stops rather than
/// failing the build, because a delay-load edge is a supplement to the normal
/// import table and never the only way a dependency is discovered.
///
/// Every read is bounds-checked through `slice::get` -- the input is an
/// arbitrary file on disk and may be truncated.
fn parse_delay_imports(bytes: &[u8], pe: &goblin::pe::PE) -> Vec<String> {
    let Some(optional_header) = pe.header.optional_header else {
        return Vec::new();
    };
    let Some(directory) = optional_header
        .data_directories
        .get_delay_import_descriptor()
    else {
        return Vec::new();
    };
    if directory.size == 0 || directory.virtual_address == 0 {
        return Vec::new();
    }

    let Some(table_start) = rva_to_offset(&pe.sections, directory.virtual_address) else {
        warn!(
            "  Delay-load directory RVA {:#x} is outside every section; skipping delay imports",
            directory.virtual_address
        );
        return Vec::new();
    };

    let mut names: Vec<String> = Vec::new();
    let descriptor_count = directory.size as usize / SIZEOF_DELAYLOAD_DESCRIPTOR;
    for index in 0..descriptor_count {
        let base = table_start + index * SIZEOF_DELAYLOAD_DESCRIPTOR;
        let Some(attributes) = read_u32(bytes, base) else {
            warn!("  Truncated delay-load descriptor at offset {base:#x}; stopping");
            break;
        };
        let Some(name_address) = read_u32(bytes, base + 4) else {
            warn!("  Truncated delay-load descriptor at offset {base:#x}; stopping");
            break;
        };
        // The table is terminated by an all-zero descriptor.
        if attributes == 0 && name_address == 0 {
            break;
        }
        if name_address == 0 {
            continue;
        }

        let name_rva = if attributes & DELAYLOAD_ATTR_RVA != 0 {
            name_address
        } else {
            // Pre-VC6 form: absolute virtual addresses. Convert to an RVA.
            warn!("  Delay-load descriptor uses legacy absolute addresses");
            match (name_address as u64).checked_sub(pe.image_base as u64) {
                Some(rva) if rva <= u32::MAX as u64 => rva as u32,
                _ => continue,
            }
        };

        let Some(name_offset) = rva_to_offset(&pe.sections, name_rva) else {
            warn!("  Delay-load name RVA {name_rva:#x} is outside every section; skipping");
            continue;
        };
        match read_cstr(bytes, name_offset) {
            Some(name) if !name.is_empty() => names.push(name),
            Some(_) => {}
            None => warn!("  Unterminated delay-load name at offset {name_offset:#x}"),
        }
    }
    names
}

/// Translate an RVA to a file offset using the section table.
///
/// Hand-rolled rather than using `goblin::pe::utils::find_offset`, whose
/// signature has changed between goblin versions.
fn rva_to_offset(sections: &[goblin::pe::section_table::SectionTable], rva: u32) -> Option<usize> {
    for section in sections {
        // A section's in-memory size can exceed its on-disk size (BSS-like
        // padding), so use the larger for the range test but refuse offsets
        // that land in the part with no file backing.
        let mapped_size = section.size_of_raw_data.max(section.virtual_size);
        let start = section.virtual_address;
        let end = start.checked_add(mapped_size)?;
        if rva >= start && rva < end {
            let delta = rva - start;
            if delta >= section.size_of_raw_data {
                return None;
            }
            return Some(section.pointer_to_raw_data as usize + delta as usize);
        }
    }
    None
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    bytes
        .get(offset..offset.checked_add(4)?)
        .and_then(|b| <[u8; 4]>::try_from(b).ok())
        .map(u32::from_le_bytes)
}

/// Read a NUL-terminated ASCII string, refusing to run off the end.
fn read_cstr(bytes: &[u8], offset: usize) -> Option<String> {
    let tail = bytes.get(offset..)?;
    let end = tail.iter().position(|&b| b == 0)?;
    Some(String::from_utf8_lossy(&tail[..end]).into_owned())
}

/// Whether a library name is an API set rather than a real file.
///
/// API sets (`api-ms-*`, `ext-ms-*`) are resolved by the loader through the API
/// set schema in the PEB and have no file on disk -- although `System32` does
/// contain real forwarder files for some of them on some Windows SKUs, which is
/// why this check has to happen before any filesystem lookup.
///
/// The prefix is deliberately `api-ms-` / `ext-ms-` and not the narrower
/// `api-ms-win-` / `ext-ms-win-`: real traversals hit `ext-ms-onecore-*`,
/// `ext-ms-mf-pal-*` and `ext-ms-windowscore-*`, each of which would otherwise
/// become an unfixable build failure. It is also no wider than that, because
/// over-matching would silently drop a real dependency with no build error at
/// all -- the worst available failure mode.
pub fn is_api_set(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.starts_with("api-ms-") || lower.starts_with("ext-ms-")
}

/// The Windows [`BinaryScanner`].
///
/// Resolution is context-free: a PE import is a bare file name and the search
/// order does not depend on the importing binary. `Context` is therefore `()`
/// and both `chain` and `ResolveEnv::search_dirs` are ignored -- the search
/// directories are pre-indexed at construction instead.
pub struct WindowsScanner {
    search_dirs: Vec<IndexedDir>,
    /// Machine type every bundled binary must have. Mismatches are rejected so
    /// that accidentally bundling a 32-bit DLL becomes visible.
    expected_machine: Option<u16>,
}

impl WindowsScanner {
    pub fn new(search_dirs: &[(PathBuf, SearchDirKind)], expected_machine: Option<u16>) -> Self {
        let indexed: Vec<IndexedDir> = search_dirs
            .iter()
            .map(|(path, kind)| {
                let dir = IndexedDir::build(path, *kind);
                debug!(
                    "  Indexed search dir {:?} ({} files, {:?})",
                    dir.path,
                    dir.by_name.len(),
                    dir.kind
                );
                dir
            })
            .collect();
        Self {
            search_dirs: indexed,
            expected_machine,
        }
    }

    /// Search directories in priority order, for reporting.
    pub fn search_dir_summary(&self) -> Vec<(PathBuf, SearchDirKind, usize)> {
        self.search_dirs
            .iter()
            .map(|d| (d.path.clone(), d.kind, d.by_name.len()))
            .collect()
    }
}

fn file_name_key(name: &str) -> String {
    // References are bare file names, but be tolerant of a path just in case.
    let normalized = name.replace('\\', "/");
    normalized
        .rsplit('/')
        .next()
        .unwrap_or(&normalized)
        .to_lowercase()
}

impl BinaryScanner for WindowsScanner {
    type Context = ();

    fn format_name(&self) -> &'static str {
        "PE"
    }

    fn is_binary(&self, path: &Path) -> bool {
        has_pe_extension(path)
    }

    fn scan(&self, binary: &Path) -> Result<Option<ScannedBinary<()>>, anyhow::Error> {
        let Some(imports) = read_pe_imports(binary)? else {
            return Ok(None);
        };
        Ok(Some(ScannedBinary {
            references: imports.all().map(LibraryReference::new).collect(),
            context: (),
        }))
    }

    fn reference_key(&self, reference: &LibraryReference) -> String {
        file_name_key(&reference.raw)
    }

    fn path_key(&self, path: &Path) -> String {
        path.file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    }

    fn provided_reason(&self, reference: &LibraryReference) -> Option<String> {
        is_api_set(&reference.raw).then(|| String::from("Windows API set"))
    }

    fn resolve(
        &self,
        reference: &LibraryReference,
        _chain: &[()],
        _env: &ResolveEnv,
    ) -> Resolution {
        let key = self.reference_key(reference);
        let mut tried: Vec<PathBuf> = Vec::new();

        for dir in &self.search_dirs {
            let Some(candidate) = dir.by_name.get(&key) else {
                tried.push(dir.path.join(&key));
                continue;
            };
            if let (Some(expected), Some(actual)) = (self.expected_machine, machine_of(candidate)) {
                if expected != actual {
                    warn!(
                        "  Ignoring {:?}: machine {:#x} does not match the executable's {:#x}",
                        candidate, actual, expected
                    );
                    tried.push(candidate.clone());
                    continue;
                }
            }
            if dir.kind == SearchDirKind::System {
                // Bundling something out of a system directory is intentional
                // for a handful of redistributables, but a *new* one appearing
                // should be noticed.
                info!(
                    "  Resolved {} from system directory {:?}",
                    reference.raw, dir.path
                );
            }
            return Resolution::Found(candidate.clone());
        }

        Resolution::Unresolvable { tried }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_set_prefixes_that_appear_in_real_traversals() {
        // The narrower `api-ms-win-` / `ext-ms-win-` prefix would miss all of
        // these, and each one would become a fatal "not in include list" error.
        for name in [
            "api-ms-win-crt-heap-l1-1-0.dll",
            "API-MS-WIN-CORE-FILE-L1-1-0.DLL",
            "ext-ms-win32-subsystem-query-l1-1-0.dll",
            "ext-ms-onecore-dcomp-l1-1-0.dll",
            "ext-ms-onecore-appmodel-statemanager-l1-1-0.dll",
            "ext-ms-mf-pal-l2-1-0.dll",
            "ext-ms-windowscore-deviceinfo-l1-1-0.dll",
        ] {
            assert!(is_api_set(name), "{name} must be treated as an API set");
        }
    }

    #[test]
    fn api_set_check_does_not_over_match() {
        // Over-matching would silently drop a real dependency with no build
        // error, so the prefix must stay narrow.
        for name in [
            "apiary.dll",
            "extra.dll",
            "api.dll",
            "extension.dll",
            "kernel32.dll",
            "Qt6Core.dll",
            "",
        ] {
            assert!(
                !is_api_set(name),
                "{name} must NOT be treated as an API set"
            );
        }
    }

    #[test]
    fn extension_filter_is_case_insensitive() {
        assert!(has_pe_extension(Path::new("a/b/Qt6Core.dll")));
        // The package really does ship this name.
        assert!(has_pe_extension(Path::new("a/b/libmp3lame.DLL")));
        assert!(has_pe_extension(Path::new("a/b/app.EXE")));
        assert!(!has_pe_extension(Path::new("a/b/Qt6Core.pdb")));
        assert!(!has_pe_extension(Path::new("a/b/icon.png")));
        assert!(!has_pe_extension(Path::new("a/b/qmldir")));
    }

    #[test]
    fn reference_keys_are_lowercased_file_names() {
        let scanner = WindowsScanner::new(&[], None);
        assert_eq!(
            scanner.reference_key(&LibraryReference::new("KERNEL32.dll")),
            "kernel32.dll"
        );
        // Tolerant of a path, though real PE imports are bare names.
        assert_eq!(
            scanner.reference_key(&LibraryReference::new("SubDir\\Qt6Core.dll")),
            "qt6core.dll"
        );
    }

    #[test]
    fn non_pe_bytes_are_rejected_without_erroring() {
        assert!(parse_pe_imports(b"not a PE file at all").is_none());
        assert!(parse_pe_imports(&[]).is_none());
        // Right magic, but far too short to be a real image.
        assert!(parse_pe_imports(b"MZ").is_none());
    }

    #[test]
    fn a_text_file_is_not_a_binary() {
        // Must be Ok(None), not Err: the walker treats Err as a build failure.
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let result = read_pe_imports(&manifest).expect("reading a text file must not error");
        assert!(result.is_none());
    }

    /// The test binary is itself a PE image, so goblin integration can be
    /// exercised without committing a binary fixture.
    #[test]
    #[cfg(target_os = "windows")]
    fn reads_imports_of_the_test_binary_itself() {
        let exe = std::env::current_exe().expect("current exe");
        let imports = read_pe_imports(&exe)
            .expect("must parse")
            .expect("the test binary is a PE image");

        assert_eq!(imports.machine, goblin::pe::header::COFF_MACHINE_X86_64);
        assert!(
            imports
                .all()
                .any(|l| l.eq_ignore_ascii_case("kernel32.dll") || is_api_set(l)),
            "expected a CRT or kernel32 import, got {:?}",
            imports.libraries
        );
    }

    #[test]
    fn rva_outside_every_section_is_rejected() {
        let sections = vec![goblin::pe::section_table::SectionTable {
            name: *b".text\0\0\0",
            real_name: None,
            virtual_size: 0x100,
            virtual_address: 0x1000,
            size_of_raw_data: 0x100,
            pointer_to_raw_data: 0x400,
            pointer_to_relocations: 0,
            pointer_to_linenumbers: 0,
            number_of_relocations: 0,
            number_of_linenumbers: 0,
            characteristics: 0,
        }];

        assert_eq!(rva_to_offset(&sections, 0x1000), Some(0x400));
        assert_eq!(rva_to_offset(&sections, 0x1050), Some(0x450));
        assert_eq!(rva_to_offset(&sections, 0x0999), None);
        assert_eq!(rva_to_offset(&sections, 0x2000), None);
    }

    /// An RVA inside a section's virtual padding has no file backing, so it must
    /// not produce an offset that reads unrelated bytes.
    #[test]
    fn rva_in_virtual_padding_has_no_file_offset() {
        let sections = vec![goblin::pe::section_table::SectionTable {
            name: *b".data\0\0\0",
            real_name: None,
            virtual_size: 0x1000,
            virtual_address: 0x1000,
            size_of_raw_data: 0x10,
            pointer_to_raw_data: 0x400,
            pointer_to_relocations: 0,
            number_of_relocations: 0,
            pointer_to_linenumbers: 0,
            number_of_linenumbers: 0,
            characteristics: 0,
        }];

        assert_eq!(rva_to_offset(&sections, 0x1000), Some(0x400));
        assert_eq!(rva_to_offset(&sections, 0x1010), None);
    }

    #[test]
    fn cstr_and_u32_reads_are_bounds_checked() {
        let bytes = b"abc\0\x01\x02\x03\x04";
        assert_eq!(read_cstr(bytes, 0).as_deref(), Some("abc"));
        assert_eq!(
            read_cstr(bytes, 4),
            None,
            "no NUL terminator after offset 4"
        );
        assert_eq!(read_cstr(bytes, 999), None);
        assert_eq!(read_u32(bytes, 4), Some(0x0403_0201));
        assert_eq!(read_u32(bytes, 6), None);
        assert_eq!(read_u32(bytes, usize::MAX), None);
    }
}
