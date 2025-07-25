#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
    }

    unsafe extern "RustQt" {
    }

    unsafe extern "C++" {
    }
}

pub use ffi::CompositeLoop;

pub struct CompositeLoopRust {}

impl Default for CompositeLoopRust {
    fn default() -> CompositeLoopRust {
        CompositeLoopRust {}
    }
}
