pub mod dependencies;

#[cfg(target_os = "linux")]
pub mod linux_appimage;

#[cfg(target_os = "macos")]
pub mod macos_appbundle;

