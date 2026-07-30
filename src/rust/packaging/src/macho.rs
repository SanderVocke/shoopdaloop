//! Mach-O load-command reading for the macOS dependency walker.
//!
//! Replaces the `otool`-based `scripts/macos_deps.sh`. Beyond removing one
//! subprocess per binary, reading load commands in process is what makes it
//! possible to use *each* binary's own `LC_RPATH` when resolving its
//! dependencies. The shell script read `LC_RPATH` only from the root executable
//! and applied it to everything, which is not what dyld does.
//!
//! This module is pure byte parsing with no macOS-specific APIs, so it compiles
//! and its tests run on any host.

use anyhow::Context;
use std::path::Path;

use common::logging::macros::*;
shoop_log_unit!("packaging");

pub const MH_MAGIC: u32 = 0xfeed_face;
pub const MH_CIGAM: u32 = 0xcefa_edfe;
pub const MH_MAGIC_64: u32 = 0xfeed_facf;
pub const MH_CIGAM_64: u32 = 0xcffa_edfe;
pub const FAT_MAGIC: u32 = 0xcafe_babe;
pub const FAT_CIGAM: u32 = 0xbeba_feca;

/// Loadable image types. Anything else is not something the loader would pull
/// in as a dependency, and must not be treated as a scan root.
///
/// Excluding `MH_DSYM` matters in practice: a `.dSYM` payload is a perfectly
/// parseable Mach-O with real load commands, so seeding one would make
/// `@loader_path` resolve relative to the dSYM directory and produce a pile of
/// unresolvable references.
const LOADABLE_FILETYPES: &[u32] = &[
    goblin::mach::header::MH_EXECUTE,
    goblin::mach::header::MH_DYLIB,
    goblin::mach::header::MH_BUNDLE,
];

/// Cheap magic-number test on the first four bytes.
///
/// `FAT_MAGIC` collides with the Java class-file magic, which is acceptable:
/// [`read_macho`] then attempts a real parse and returns `Ok(None)` if it is not
/// actually Mach-O.
pub fn looks_like_macho(first_bytes: &[u8]) -> bool {
    let Some(raw) = first_bytes.get(0..4) else {
        return false;
    };
    let value = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
    matches!(
        value,
        MH_MAGIC | MH_CIGAM | MH_MAGIC_64 | MH_CIGAM_64 | FAT_MAGIC | FAT_CIGAM
    )
}

/// Whether `path` starts with a Mach-O magic number.
///
/// False for directories, unreadable files, and anything shorter than 4 bytes.
/// Unlike a filename check this actually works on macOS, where neither the
/// executable nor Qt's plugins carry a distinguishing extension.
pub fn is_macho_file(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut head = [0u8; 4];
    match file.read_exact(&mut head) {
        Ok(()) => looks_like_macho(&head),
        Err(_) => false,
    }
}

/// What one Mach-O image declares.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MachoInfo {
    /// `LC_ID_DYLIB`, i.e. the name the image expects to be loaded under.
    pub install_name: Option<String>,
    pub filetype: u32,
    /// `LC_LOAD_DYLIB`, `LC_LOAD_WEAK_DYLIB`, `LC_REEXPORT_DYLIB`,
    /// `LC_LAZY_LOAD_DYLIB` and `LC_LOAD_UPWARD_DYLIB` targets, verbatim, in
    /// file order, deduplicated.
    pub libs: Vec<String>,
    /// `LC_RPATH` entries, verbatim (so still containing `@loader_path` etc).
    pub rpaths: Vec<String>,
}

/// Parse Mach-O bytes.
///
/// `Ok(None)` if the bytes are not Mach-O, or are a Mach-O whose filetype is not
/// a loadable image. For fat binaries the slices are unioned: they should agree,
/// and taking the union is the safe direction to be wrong in.
pub fn read_macho(bytes: &[u8]) -> Result<Option<MachoInfo>, anyhow::Error> {
    if !looks_like_macho(bytes) {
        return Ok(None);
    }
    let Ok(mach) = goblin::mach::Mach::parse(bytes) else {
        return Ok(None);
    };
    match mach {
        goblin::mach::Mach::Binary(macho) => Ok(from_macho(&macho)),
        goblin::mach::Mach::Fat(multi) => {
            let mut merged: Option<MachoInfo> = None;
            let arch_count = multi.arches().map(|a| a.len()).unwrap_or(0);
            for index in 0..arch_count {
                let Ok(goblin::mach::SingleArch::MachO(macho)) = multi.get(index) else {
                    // Archive slices and unparseable slices are not loadable
                    // images; skip them rather than failing the whole file.
                    continue;
                };
                let Some(info) = from_macho(&macho) else {
                    continue;
                };
                match merged.as_mut() {
                    None => merged = Some(info),
                    Some(existing) => {
                        for lib in info.libs {
                            if !existing.libs.contains(&lib) {
                                existing.libs.push(lib);
                            }
                        }
                        for rpath in info.rpaths {
                            if !existing.rpaths.contains(&rpath) {
                                existing.rpaths.push(rpath);
                            }
                        }
                        if existing.install_name.is_none() {
                            existing.install_name = info.install_name;
                        }
                    }
                }
            }
            Ok(merged)
        }
    }
}

fn from_macho(macho: &goblin::mach::MachO) -> Option<MachoInfo> {
    if !LOADABLE_FILETYPES.contains(&macho.header.filetype) {
        debug!(
            "  Mach-O filetype {:#x} is not a loadable image; skipping",
            macho.header.filetype
        );
        return None;
    }

    // goblin seeds `libs` with the literal string "self" and then overwrites
    // index 0 with LC_ID_DYLIB if the image has one. Either way index 0 is the
    // image's own identity, never a dependency, and must be dropped -- otherwise
    // every dylib would appear to depend on itself and every executable on a
    // library called "self".
    let libs: Vec<String> = macho
        .libs
        .iter()
        .skip(1)
        .map(|s| s.to_string())
        .fold(Vec::new(), |mut acc, lib| {
            if !acc.contains(&lib) {
                acc.push(lib);
            }
            acc
        });

    Some(MachoInfo {
        install_name: macho.name.map(str::to_string),
        filetype: macho.header.filetype,
        libs,
        rpaths: macho.rpaths.iter().map(|s| s.to_string()).collect(),
    })
}

pub fn read_macho_file(path: &Path) -> Result<Option<MachoInfo>, anyhow::Error> {
    let bytes =
        std::fs::read(path).with_context(|| format!("Cannot read {}", path.display()))?;
    read_macho(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LC_REQ_DYLD: u32 = 0x8000_0000;
    const LC_LOAD_DYLIB: u32 = 0xc;
    const LC_ID_DYLIB: u32 = 0xd;
    const LC_LOAD_WEAK_DYLIB: u32 = 0x18 | LC_REQ_DYLD;
    const LC_RPATH: u32 = 0x1c | LC_REQ_DYLD;
    const LC_REEXPORT_DYLIB: u32 = 0x1f | LC_REQ_DYLD;
    const LC_LAZY_LOAD_DYLIB: u32 = 0x20;
    const LC_LOAD_UPWARD_DYLIB: u32 = 0x23 | LC_REQ_DYLD;

    const CPU_TYPE_X86_64: u32 = 0x0100_0007;
    const CPU_TYPE_ARM64: u32 = 0x0100_000c;

    /// A dylib-style load command: 24-byte header then the NUL-terminated,
    /// 8-byte-aligned path.
    fn dylib_command(cmd: u32, name: &str) -> Vec<u8> {
        let mut payload = name.as_bytes().to_vec();
        payload.push(0);
        while (24 + payload.len()) % 8 != 0 {
            payload.push(0);
        }
        let cmdsize = (24 + payload.len()) as u32;
        let mut out = Vec::new();
        out.extend_from_slice(&cmd.to_le_bytes());
        out.extend_from_slice(&cmdsize.to_le_bytes());
        out.extend_from_slice(&24u32.to_le_bytes()); // dylib.name offset
        out.extend_from_slice(&0u32.to_le_bytes()); // timestamp
        out.extend_from_slice(&0u32.to_le_bytes()); // current_version
        out.extend_from_slice(&0u32.to_le_bytes()); // compatibility_version
        out.extend_from_slice(&payload);
        out
    }

    /// An `LC_RPATH` command: 12-byte header then the path.
    fn rpath_command(path: &str) -> Vec<u8> {
        let mut payload = path.as_bytes().to_vec();
        payload.push(0);
        while (12 + payload.len()) % 8 != 0 {
            payload.push(0);
        }
        let cmdsize = (12 + payload.len()) as u32;
        let mut out = Vec::new();
        out.extend_from_slice(&LC_RPATH.to_le_bytes());
        out.extend_from_slice(&cmdsize.to_le_bytes());
        out.extend_from_slice(&12u32.to_le_bytes()); // path offset
        out.extend_from_slice(&payload);
        out
    }

    /// Build a 64-bit Mach-O image out of pre-encoded load commands.
    ///
    /// Synthesizing beats committing a real dylib: no opaque binary or licence
    /// note in the source tree, and -- the actual reason -- it can express the
    /// edge cases a single real file cannot, such as weak and reexport
    /// references, several rpaths, and non-loadable filetypes.
    fn synth_macho(cputype: u32, filetype: u32, commands: &[Vec<u8>]) -> Vec<u8> {
        let sizeofcmds: usize = commands.iter().map(Vec::len).sum();
        let mut out = Vec::new();
        out.extend_from_slice(&MH_MAGIC_64.to_le_bytes());
        out.extend_from_slice(&cputype.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // cpusubtype
        out.extend_from_slice(&filetype.to_le_bytes());
        out.extend_from_slice(&(commands.len() as u32).to_le_bytes());
        out.extend_from_slice(&(sizeofcmds as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // flags
        out.extend_from_slice(&0u32.to_le_bytes()); // reserved
        for command in commands {
            out.extend_from_slice(command);
        }
        out
    }

    /// Wrap slices in a universal binary. Fat headers are big-endian.
    fn synth_fat(slices: &[Vec<u8>]) -> Vec<u8> {
        let header_len = 8 + slices.len() * 20;
        let mut offsets = Vec::new();
        let mut cursor = header_len;
        for slice in slices {
            // Real fat binaries page-align slices; any alignment parses.
            cursor = (cursor + 0xfff) & !0xfff;
            offsets.push(cursor);
            cursor += slice.len();
        }

        let mut out = Vec::new();
        out.extend_from_slice(&FAT_MAGIC.to_be_bytes());
        out.extend_from_slice(&(slices.len() as u32).to_be_bytes());
        for (index, slice) in slices.iter().enumerate() {
            let cputype = if index == 0 {
                CPU_TYPE_X86_64
            } else {
                CPU_TYPE_ARM64
            };
            out.extend_from_slice(&cputype.to_be_bytes());
            out.extend_from_slice(&0u32.to_be_bytes()); // cpusubtype
            out.extend_from_slice(&(offsets[index] as u32).to_be_bytes());
            out.extend_from_slice(&(slice.len() as u32).to_be_bytes());
            out.extend_from_slice(&12u32.to_be_bytes()); // align 2^12
        }
        for (index, slice) in slices.iter().enumerate() {
            out.resize(offsets[index], 0);
            out.extend_from_slice(slice);
        }
        out
    }

    #[test]
    fn magic_sniffing() {
        assert!(looks_like_macho(&MH_MAGIC_64.to_le_bytes()));
        assert!(looks_like_macho(&MH_MAGIC.to_le_bytes()));
        assert!(looks_like_macho(&MH_CIGAM_64.to_le_bytes()));
        assert!(looks_like_macho(&FAT_MAGIC.to_le_bytes()));
        assert!(!looks_like_macho(b"MZ\x90\x00"));
        assert!(!looks_like_macho(b"\x7fELF"));
        assert!(!looks_like_macho(b"abc"));
        assert!(!looks_like_macho(&[]));
    }

    /// The single most important test in this module: goblin's `libs[0]` is the
    /// image's own identity, not a dependency.
    #[test]
    fn the_install_name_is_not_reported_as_a_dependency() {
        let bytes = synth_macho(
            CPU_TYPE_X86_64,
            goblin::mach::header::MH_DYLIB,
            &[
                dylib_command(LC_ID_DYLIB, "@rpath/libQt6Core.6.dylib"),
                dylib_command(LC_LOAD_DYLIB, "/usr/lib/libSystem.B.dylib"),
            ],
        );
        let info = read_macho(&bytes).unwrap().expect("a loadable dylib");

        assert_eq!(
            info.install_name.as_deref(),
            Some("@rpath/libQt6Core.6.dylib")
        );
        assert_eq!(info.libs, vec!["/usr/lib/libSystem.B.dylib"]);
        assert!(
            !info.libs.iter().any(|l| l.contains("libQt6Core")),
            "the image must not depend on itself"
        );
        assert!(
            !info.libs.iter().any(|l| l == "self"),
            "goblin's 'self' sentinel must never leak out"
        );
    }

    #[test]
    fn an_executable_without_an_id_dylib_drops_the_self_sentinel() {
        let bytes = synth_macho(
            CPU_TYPE_X86_64,
            goblin::mach::header::MH_EXECUTE,
            &[dylib_command(LC_LOAD_DYLIB, "@rpath/libQt6Quick.6.dylib")],
        );
        let info = read_macho(&bytes).unwrap().expect("a loadable executable");
        assert_eq!(info.libs, vec!["@rpath/libQt6Quick.6.dylib"]);
    }

    /// All five load-dylib kinds land in one goblin vec, so all five have to be
    /// picked up -- a reexported or weakly-linked Qt module is still a file that
    /// has to be in the bundle.
    #[test]
    fn every_load_dylib_variant_is_collected() {
        let bytes = synth_macho(
            CPU_TYPE_X86_64,
            goblin::mach::header::MH_BUNDLE,
            &[
                dylib_command(LC_LOAD_DYLIB, "libnormal.dylib"),
                dylib_command(LC_LOAD_WEAK_DYLIB, "libweak.dylib"),
                dylib_command(LC_REEXPORT_DYLIB, "libreexport.dylib"),
                dylib_command(LC_LAZY_LOAD_DYLIB, "liblazy.dylib"),
                dylib_command(LC_LOAD_UPWARD_DYLIB, "libupward.dylib"),
            ],
        );
        let info = read_macho(&bytes).unwrap().expect("a loadable bundle");
        assert_eq!(
            info.libs,
            vec![
                "libnormal.dylib",
                "libweak.dylib",
                "libreexport.dylib",
                "liblazy.dylib",
                "libupward.dylib",
            ]
        );
    }

    #[test]
    fn rpaths_are_collected_verbatim_and_in_order() {
        let bytes = synth_macho(
            CPU_TYPE_X86_64,
            goblin::mach::header::MH_DYLIB,
            &[
                rpath_command("@loader_path/../../lib"),
                rpath_command("@executable_path/lib"),
                rpath_command("/opt/vcpkg/lib"),
                dylib_command(LC_LOAD_DYLIB, "@rpath/libfoo.dylib"),
            ],
        );
        let info = read_macho(&bytes).unwrap().expect("a loadable dylib");
        assert_eq!(
            info.rpaths,
            vec![
                "@loader_path/../../lib",
                "@executable_path/lib",
                "/opt/vcpkg/lib",
            ],
            "rpaths must stay unexpanded and ordered -- resolution order depends on it"
        );
    }

    #[test]
    fn non_loadable_filetypes_are_rejected() {
        for filetype in [
            goblin::mach::header::MH_OBJECT,
            goblin::mach::header::MH_DSYM,
        ] {
            let bytes = synth_macho(
                CPU_TYPE_X86_64,
                filetype,
                &[dylib_command(LC_LOAD_DYLIB, "libfoo.dylib")],
            );
            assert!(
                read_macho(&bytes).unwrap().is_none(),
                "filetype {filetype:#x} must not be treated as a loadable image"
            );
        }
    }

    #[test]
    fn fat_binaries_union_their_slices() {
        let x86 = synth_macho(
            CPU_TYPE_X86_64,
            goblin::mach::header::MH_DYLIB,
            &[
                dylib_command(LC_ID_DYLIB, "@rpath/libboth.dylib"),
                dylib_command(LC_LOAD_DYLIB, "libcommon.dylib"),
                dylib_command(LC_LOAD_DYLIB, "libx86only.dylib"),
            ],
        );
        let arm = synth_macho(
            CPU_TYPE_ARM64,
            goblin::mach::header::MH_DYLIB,
            &[
                dylib_command(LC_ID_DYLIB, "@rpath/libboth.dylib"),
                dylib_command(LC_LOAD_DYLIB, "libcommon.dylib"),
                dylib_command(LC_LOAD_DYLIB, "libarmonly.dylib"),
            ],
        );
        let fat = synth_fat(&[x86, arm]);

        let info = read_macho(&fat).unwrap().expect("a loadable fat dylib");
        assert!(info.libs.contains(&String::from("libcommon.dylib")));
        assert!(info.libs.contains(&String::from("libx86only.dylib")));
        assert!(info.libs.contains(&String::from("libarmonly.dylib")));
        assert_eq!(
            info.libs.iter().filter(|l| *l == "libcommon.dylib").count(),
            1,
            "the union must not duplicate a shared dependency"
        );
    }

    #[test]
    fn non_macho_bytes_are_not_an_error() {
        assert!(read_macho(b"not a Mach-O").unwrap().is_none());
        assert!(read_macho(&[]).unwrap().is_none());
        // Right magic, garbage body.
        let mut truncated = MH_MAGIC_64.to_le_bytes().to_vec();
        truncated.extend_from_slice(b"junk");
        assert!(read_macho(&truncated).unwrap().is_none());
    }

    #[test]
    fn a_text_file_is_not_a_macho_file() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        assert!(!is_macho_file(&manifest));
        assert!(read_macho_file(&manifest).unwrap().is_none());
    }
}
