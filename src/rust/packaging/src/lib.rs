pub mod dependencies;
pub mod fs_helpers;

#[cfg(target_os = "linux")]
pub mod linux_appdir;
#[cfg(target_os = "linux")]
pub mod linux_appimage;

#[cfg(target_os = "macos")]
pub mod macos_appbundle;

