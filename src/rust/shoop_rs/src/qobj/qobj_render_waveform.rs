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

    // QLinF API
    unsafe extern "C++" {
        include!(<QtCore/QLineF>);
        type QLineF;
    }

    // QPen API
    unsafe extern "C++" {
        include!(<QtGui/QPen>);
        type QPen;
    }

    // QList API
    unsafe extern "C++" {
        include!("cxx-qt-lib/qlist.h");
        type QList_QLine = cxx_qt_lib::QList<QLineF>;
    }

    // QPainter API
    unsafe extern "C++" {
        type QPainter;
        include!(<QtGui/QPainter>);

        #[rust_name = "set_pen"]
        fn setPen(self : Pin<&mut QPainter>, pen : &QPen);

        #[rust_name = "draw_lines"]
        fn drawLines(self : Pin<&mut QPainter>, points : &QList_QLine);
    }

    // Define the API from QtQuick that we need
    unsafe extern "C++" {
        /// Define QQuickItem as a type
        type QQuickItem;
        include!(<QtQuick/QQuickItem>);

        include!(<QtQuick/QQuickPaintedItem>);
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = "QQuickPaintedItem"]
        #[qproperty(QColor, color)]
        type CustomParentClass = super::CustomParentClassRust;

        /// Override QQuickPaintedItem::paint to draw two rectangles in Rust using QPainter
        #[qinvokable]
        #[cxx_override]
        unsafe fn paint(self: Pin<&mut CustomParentClass>, painter: *mut QPainter);

        /// Define that we need to inherit size() from the base class
        #[inherit]
        fn size(self: &CustomParentClass) -> QSizeF;

        /// Define that we need to inherit update() from the base class
        #[inherit]
        fn update(self: Pin<&mut CustomParentClass>);
    }

    impl cxx_qt::Constructor<()> for CustomParentClass {}
}

use core::pin::Pin;
use cxx_qt_lib::{QColor, QRectF};

/// A struct which inherits from QQuickPaintedItem
///
/// Which has a parent of the type QQuickItem rather than QObject.
#[derive(Default)]
pub struct CustomParentClassRust {
    color: QColor,
}

impl qobject::CustomParentClass {
    /// Override QQuickPaintedItem::paint to draw two rectangles in Rust using QPainter
    ///
    /// # Safety
    ///
    /// As we deref a pointer in a public method this needs to be marked as unsafe
    pub unsafe fn paint(self: Pin<&mut Self>, painter: *mut qobject::QPainter) {
        // We need to convert the *mut QPainter to a Pin<&mut QPainter> so that we can reach the methods
        if let Some(painter) = painter.as_mut() {
            let mut pinned_painter = Pin::new_unchecked(painter);

            // Now pinned painter can be used as normal
            // to render a rectangle with two colours
            let size = self.as_ref().size();
            pinned_painter.as_mut().fill_rect(
                &QRectF::new(0.0, 0.0, size.width() / 2.0, size.height()),
                self.as_ref().color(),
            );
            let darker_color = self.as_ref().color().darker(150);
            pinned_painter.as_mut().fill_rect(
                &QRectF::new(size.width() / 2.0, 0.0, size.width() / 2.0, size.height()),
                &darker_color,
            );
        }
    }
}

impl cxx_qt::Initialize for qobject::CustomParentClass {
    fn initialize(self: core::pin::Pin<&mut Self>) {
        self.on_color_changed(|qobject| qobject.update()).release();
    }
}
