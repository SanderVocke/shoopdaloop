use cxx::{type_id, ExternType};
use cxx_qt;
use std::pin::Pin;

#[cxx_qt::bridge(cxx_file_stem="qpen")]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qpen.h");
        type QPen = super::QPenRust;
        type QColor = cxx_qt_lib::QColor;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qpen.h");
        
        #[rust_name = "qpen_set_color"]
        fn qpenSetColor(pen : Pin<&mut QPen>, color : &QColor);
    }

    unsafe extern "C++" {
        include!("cxx-qt-shoop/make_raw.h");

        #[rust_name = "qpen_make_raw"]
        unsafe fn make_raw() -> *mut QPen;
    }

    extern "RustQt" {
        #[qobject]
        type DummyQPen = super::DummyQPenRust;
    }
}

#[derive(Default)]
pub struct DummyQPenRust {}

#[repr(C)]
pub struct QPenRust {
}

pub use ffi::QPen;

impl QPen {
    pub fn set_color(pen : Pin<&mut QPen>, color : &ffi::QColor) {
        ffi::qpen_set_color(pen, color);
    }

    pub fn make_boxed() -> Box<QPen> {
        let raw = unsafe { ffi::qpen_make_raw() };
        unsafe { Box::from_raw(raw) }.into()
    }
}

unsafe impl ExternType for QPen {
    type Id = type_id!("QPen");
    type Kind = cxx::kind::Opaque;
}
