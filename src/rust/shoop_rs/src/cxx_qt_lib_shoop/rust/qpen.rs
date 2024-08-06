use cxx::{type_id, ExternType};

#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qpen.h");
        type QPen = super::QPen;
        type QColor = cxx_qt_lib::QColor;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qpen.h");

        #[rust_name = "qpen_default"]
        fn qpenDefault() -> QPen;

        #[rust_name = "qpen_from_color"]
        fn qpenFromColor(color : &QColor) -> QPen;
    }
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct QPen {
}

impl QPen {
    pub fn from_color(color : &ffi::QColor) -> Self {
        ffi::qpen_from_color(color)
    }
}

impl Default for QPen {
    fn default() -> Self {
        ffi::qpen_default()
    }
}

unsafe impl ExternType for QPen {
    type Id = type_id!("QPen");
    type Kind = cxx::kind::Trivial;
}