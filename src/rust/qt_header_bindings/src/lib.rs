#[cfg(not(feature = "prebuild"))]
mod ffi;

#[cfg(not(feature = "prebuild"))]
pub use ffi::{Qt_Key, Qt_KeyboardModifier};
