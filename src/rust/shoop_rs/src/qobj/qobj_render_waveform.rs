#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qcolor.h");
        type QColor = cxx_qt_lib::QColor;

        include!("cxx-qt-lib/qrectf.h");
        type QRectF = cxx_qt_lib::QRectF;

        include!("cxx-qt-lib/qsizef.h");
        type QSizeF = cxx_qt_lib::QSizeF;
    }

    unsafe extern "C++" {
        include!(<QtCore/QLineF>);
        type QLineF = crate::cxx_qt_ext::qlinef::QLineF;
    }

    unsafe extern "C++" {
        include!(<QtGui/QPen>);
        type QPen;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qlist.h");
        type QList_QLineF = crate::cxx_qt_ext::qlinef::QList_QLineF;
    }

    // QPainter API
    unsafe extern "C++" {
        type QPainter;
        include!(<QtGui/QPainter>);

        #[rust_name = "set_pen"]
        fn setPen(self : Pin<&mut QPainter>, pen : &QPen);

        #[rust_name = "draw_lines"]
        fn drawLines(self : Pin<&mut QPainter>, points : &QList_QLineF);
    }

    unsafe extern "C++" {
        include!(<QtQuick/QQuickPaintedItem>);
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = "QQuickPaintedItem"]
        type CustomParentClass = super::CustomParentClassRust;

        #[qinvokable]
        #[cxx_override]
        unsafe fn paint(self: Pin<&mut CustomParentClass>, painter: *mut QPainter);

        #[inherit]
        fn size(self: &CustomParentClass) -> QSizeF;

        #[inherit]
        fn update(self: Pin<&mut CustomParentClass>);
    }
}

use core::pin::Pin;
// use cxx_qt_lib::{QColor};

#[derive(Default)]
pub struct CustomParentClassRust {
}

impl qobject::CustomParentClass {
    pub unsafe fn paint(self: Pin<&mut Self>, _painter: *mut qobject::QPainter) {
    }
}