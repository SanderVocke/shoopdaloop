use common::logging::macros::*;
shoop_log_unit!("Frontend.RenderMidiSequence");

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qcolor.h");
        type QColor = cxx_qt_lib::QColor;

        include!("cxx-qt-lib/qrectf.h");
        type QRectF = cxx_qt_lib::QRectF;

        include!("cxx-qt-lib/qsizef.h");
        type QSizeF = cxx_qt_lib::QSizeF;

        include!("cxx-qt-lib/qlist.h");
        type QList_f64 = cxx_qt_lib::QList<f64>;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qlinef.h");
        type QLineF = cxx_qt_lib::QLineF;

        include!("cxx-qt-lib/qpen.h");
        type QPen = cxx_qt_lib::QPen;

        include!("cxx-qt-lib/qlist.h");
        type QList_QLineF = cxx_qt_lib::QList<cxx_qt_lib::QLineF>;

        include!("cxx-qt-lib/qvector.h");
        type QVector_QLineF = cxx_qt_lib::QVector<cxx_qt_lib::QLineF>;
        type QVector_QVariant = cxx_qt_lib::QVector<QVariant>;

        include!("cxx-qt-lib/qpainter.h");
        type QPainter = cxx_qt_lib::QPainter;

        include!(<QtQuick/QQuickPaintedItem>);
        type QQuickPaintedItem;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QQuickPaintedItem]
        #[qproperty(QVariant, input_data)]
        #[qproperty(f32, samples_per_bin)]
        #[qproperty(i32, samples_offset)]
        #[qproperty(bool, have_data, READ=get_have_data, NOTIFY=have_data_changed)]
        type RenderMidiSequence = super::RenderMidiSequenceRust;

        #[qinvokable]
        #[cxx_override]
        pub unsafe fn paint(self: Pin<&mut RenderMidiSequence>, painter: *mut QPainter);

        #[qinvokable]
        pub unsafe fn parse(self: Pin<&mut RenderMidiSequence>);

        #[qinvokable]
        pub unsafe fn get_have_data(self: Pin<&mut RenderMidiSequence>) -> bool;

        #[qsignal]
        fn have_data_changed(self: Pin<&mut RenderMidiSequence>);

        #[inherit]
        fn update(self: Pin<&mut RenderMidiSequence>);

        #[inherit]
        #[qsignal]
        #[cxx_name = "widthChanged"]
        fn width_changed(self: Pin<&mut RenderMidiSequence>);

        #[inherit]
        fn size(self: &RenderMidiSequence) -> QSizeF;
    }

    impl cxx_qt::Constructor<()> for RenderMidiSequence {}

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/make_unique.h");

        #[rust_name = "make_unique_rendermidisequence"]
        fn make_unique() -> UniquePtr<RenderMidiSequence>;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/register_qml_type.h");

        #[rust_name = "register_qml_type_rendermidisequence"]
        unsafe fn register_qml_type(
            inference_example: *mut RenderMidiSequence,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );
    }
}

use ffi::*;

pub struct Note {
    pub start: i64,
    pub end: i64,
    pub note: u8,
    pub scaled_start: i64,
    pub scaled_end: i64,
}

pub struct RenderMidiSequenceRust {
    pub samples_offset: i32,
    pub samples_per_bin: f32,
    pub input_data: QVariant,
    pub notes: Vec<Note>,
}

impl Default for RenderMidiSequenceRust {
    fn default() -> RenderMidiSequenceRust {
        RenderMidiSequenceRust {
            notes: Vec::default(),
            samples_offset: 0,
            samples_per_bin: 1_f32,
            input_data: QVariant::default(),
        }
    }
}
