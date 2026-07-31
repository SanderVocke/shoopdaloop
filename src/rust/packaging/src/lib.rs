#![cfg(not(feature = "prebuild"))]

pub mod binaries_for_test;
pub mod dependencies;
pub mod deps_walker;
pub mod fs_helpers;
pub mod list_matcher;
// The Mach-O reader and resolver are pure byte and string handling with no
// macOS-specific APIs, so they are compiled and unit-tested on Windows too --
// which is the only way the macOS logic gets exercised before it reaches a
// macOS runner. Both are excluded on Linux, which has no goblin dependency and
// keeps its own lddtree-based scanner.
#[cfg(any(windows, target_os = "macos"))]
pub mod macho;
#[cfg(any(windows, target_os = "macos"))]
pub mod macho_resolve;
#[cfg(any(windows, target_os = "macos"))]
pub mod pe;
pub mod portable_folder_common;
pub mod remove_subpaths;
pub mod scan;

mod os_dependent_modules;
pub use os_dependent_modules::*;
