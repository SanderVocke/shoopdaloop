use cxx_qt_lib::{QListElement, QPointF};
use cxx::{type_id, ExternType};
use std::fmt;

#[cxx_qt::bridge(cxx_file_stem="qlinef")]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qlinef.h");
        type QLineF = super::QLineF;
        include!("cxx-qt-lib/qpointf.h");
        type QPointF = cxx_qt_lib::QPointF;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qlist.h");
        type QList_QLineF = cxx_qt_lib::QList<QLineF>;

        fn angle(self: &QLineF) -> f64;

        #[rust_name = "angle_to"]
        fn angleTo(self: &QLineF, line: &QLineF) -> f64;

        fn p1(self: &QLineF) -> QPointF;

        fn p2(self: &QLineF) -> QPointF;

        fn x1(self: &QLineF) -> f64;

        fn x2(self: &QLineF) -> f64;

        fn y1(self: &QLineF) -> f64;

        fn y2(self: &QLineF) -> f64;

        fn center(self: &QLineF) -> QPointF;
        fn dx(self: &QLineF) -> f64;

        fn dy(self: &QLineF) -> f64;

        #[rust_name = "is_null"]
        fn isNull(self: &QLineF) -> bool;

        fn length(self: &QLineF) -> f64;

        #[rust_name = "normal_vector"]
        fn normalVector(self: &QLineF) -> QLineF;

        #[rust_name = "point_at"]
        fn pointAt(self: &QLineF, t: f64) -> QPointF;

        #[rust_name = "set_angle"]
        fn setAngle(self: &mut QLineF, angle: f64);

        #[rust_name = "set_length"]
        fn setLength(self: &mut QLineF, length: f64);

        #[rust_name = "set_p1"]
        fn setP1(self: &mut QLineF, p1: &QPointF);

        #[rust_name = "set_p2"]
        fn setP2(self: &mut QLineF, p1: &QPointF);

        #[rust_name = "set_line"]
        fn setLine(self: &mut QLineF, x1: f64, y1: f64, x2: f64, y2: f64);

        #[rust_name = "set_points"]
        fn setPoints(self: &mut QLineF, p1: &QPointF, p2: &QPointF);

        fn translate(self: &mut QLineF, offset: &QPointF);

        fn translated(self: &QLineF, offset: &QPointF) -> QLineF;

        #[rust_name = "unit_vector"]
        fn unitVector(self: &QLineF) -> QLineF;
    }

    #[namespace = "rust::cxxqtlib1"]
    unsafe extern "C++" {
        include!("cxx-qt-lib/common.h");

        #[doc(hidden)]
        #[rust_name = "qlinef_default"]
        fn construct() -> QLineF;

        #[doc(hidden)]
        #[rust_name = "qlinef_new"]
        fn construct(pt1: QPointF, pt2: QPointF) -> QLineF;

        #[doc(hidden)]
        #[rust_name = "qlinef_to_qstring"]
        fn toQString(value: &QLineF) -> QString;
    }

    #[namespace = "rust::cxxqtlib1::qlist"]
    unsafe extern "C++" {
        #[rust_name = "reserve_qlinef"]
        fn qlistReserve(_: &mut QList_QLineF, size: isize);
        #[rust_name = "append_qlinef"]
        fn qlistAppend(_: &mut QList_QLineF, _: &QLineF);
        #[rust_name = "get_unchecked_qlinef"]
        #[allow(clippy::needless_lifetimes)]
        unsafe fn qlistGetUnchecked<'a>(set: &'a QList_QLineF, pos: isize) -> &'a QLineF;
        #[rust_name = "index_of_qlinef"]
        fn qlistIndexOf(_: &QList_QLineF, _: &QLineF) -> isize;
        #[rust_name = "insert_qlinef"]
        fn qlistInsert(_: &mut QList_QLineF, _: isize, _: &QLineF);
        #[rust_name = "len_qlinef"]
        fn qlistLen(_: &QList_QLineF) -> isize;
        #[rust_name = "remove_qlinef"]
        fn qlistRemove(_: &mut QList_QLineF, _: isize);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qlinef.h");

        #[rust_name = "clear_qlist_qlinef"]
        fn clear(list : &mut QList_QLineF);

        #[rust_name = "clone_qlist_qlinef"]
        fn clone(list : &QList_QLineF) -> QList_QLineF;

        #[rust_name = "contains_qlist_qlinef"]
        fn contains(list : &QList_QLineF, line : &QLineF) -> bool;

        #[rust_name = "create_qlist_qlinef"]
        fn create() -> QList_QLineF;

        #[rust_name = "drop_qlist_qlinef"]
        fn drop(list : &mut QList_QLineF);
    }

    extern "RustQt" {
        #[qobject]
        type DummyQLineF = super::DummyQLineFRust;
    }
}

#[derive(Default)]
pub struct DummyQLineFRust {}

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct QLineF {
    pt1: QPointF,
    pt2: QPointF,
}

impl QLineF {
    pub fn new(pt1: QPointF, pt2: QPointF) -> Self {
        ffi::qlinef_new(pt1, pt2)
    }
}

impl Default for QLineF {
    fn default() -> Self {
        ffi::qlinef_default()
    }
}

impl fmt::Display for QLineF {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", ffi::qlinef_to_qstring(self))
    }
}

unsafe impl ExternType for QLineF {
    type Id = type_id!("QLineF");
    type Kind = cxx::kind::Trivial;
}

impl QListElement for QLineF {
    type TypeId = type_id!(QList_QLineF);

    fn append(list: &mut cxx_qt_lib::QList<Self>, value: Self) {
        ffi::append_qlinef(list, &value);
    }

    fn append_clone(list: &mut cxx_qt_lib::QList<Self>, value: &Self) {
        ffi::append_qlinef(list, value);
    }

    fn clear(list: &mut cxx_qt_lib::QList<Self>) {
        ffi::clear_qlist_qlinef(list);
    }

    fn clone(list: &cxx_qt_lib::QList<Self>) -> cxx_qt_lib::QList<Self> {
        ffi::clone_qlist_qlinef(list)
    }

    fn contains(list: &cxx_qt_lib::QList<Self>, value: &Self) -> bool {
        ffi::contains_qlist_qlinef(list, value)
    }

    fn default() -> cxx_qt_lib::QList<Self> {
        ffi::create_qlist_qlinef()
    }

    fn drop(list: &mut cxx_qt_lib::QList<Self>) {
        ffi::drop_qlist_qlinef(list);
    }

    unsafe fn get_unchecked(list: &cxx_qt_lib::QList<Self>, pos: isize) -> &Self {
        ffi::get_unchecked_qlinef(list, pos)
    }

    fn index_of(list: &cxx_qt_lib::QList<Self>, value: &Self) -> isize {
        ffi::index_of_qlinef(list, value)
    }

    fn insert(list: &mut cxx_qt_lib::QList<Self>, pos: isize, value: Self) {
        ffi::insert_qlinef(list, pos, &value);
    }

    fn insert_clone(list: &mut cxx_qt_lib::QList<Self>, pos: isize, value: &Self) {
        ffi::insert_qlinef(list, pos, value);
    }

    fn len(list: &cxx_qt_lib::QList<Self>) -> isize {
        ffi::len_qlinef(list)
    }

    fn remove(list: &mut cxx_qt_lib::QList<Self>, pos: isize) {
        ffi::remove_qlinef(list, pos);
    }

    fn reserve(list: &mut cxx_qt_lib::QList<Self>, size: isize) {
        ffi::reserve_qlinef(list, size);
    }
}

pub use ffi::QList_QLineF;
