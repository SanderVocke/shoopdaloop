pub mod dependencies;
pub mod fs_helpers;
pub mod binaries_for_test;

#[cfg(target_os = "linux")]
pub mod linux_appdir;
#[cfg(target_os = "linux")]
pub mod linux_appimage;

#[cfg(target_os = "macos")]
pub mod macos_appbundle;

#[cfg(target_os = "windows")]
pub mod windows_portable_folder;

