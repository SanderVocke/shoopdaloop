use cxx::{type_id, ExternType};
use std::fmt;

#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-shoop-ext/qlinef.h");
        type QLineF = ::qlinef::QLineF;
    }

    #[namespace = "rust::cxx_shoop_ext"]
    unsafe extern "C++" {
        // #[rust_name = "qlinef_dot_product"]
        // fn qlinefDotProduct(p1: &QLineF, p2: &QLineF) -> f64;
    }

    #[namespace = "rust::cxxqtlib1"]
    unsafe extern "C++" {
        include!("cxx-qt-lib/common.h");

        #[rust_name = "qlinef_init_default"]
        fn construct() -> QLineF;

        #[rust_name = "qlinef_init"]
        fn construct(x: f64, y: f64) -> QLineF;

        #[rust_name = "qlinef_from_qpoint"]
        fn construct(point: &QPoint) -> QLineF;

        #[rust_name = "qlinef_to_qstring"]
        fn toQString(value: &QLineF) -> QString;

        #[rust_name = "qlinef_plus"]
        fn operatorPlus(a: &QLineF, b: &QLineF) -> QLineF;

        #[rust_name = "qlinef_minus"]
        fn operatorMinus(a: &QLineF, b: &QLineF) -> QLineF;

        #[rust_name = "qlinef_mul"]
        fn operatorMul(a: f64, b: &QLineF) -> QLineF;

        #[rust_name = "qlinef_div"]
        fn operatorDiv(a: f64, b: &QLineF) -> QLineF;
    }
}

/// The QLineF struct defines a point in the plane using floating point precision.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct QLineF {
    x: f64,
    y: f64,
}

impl QLineF {
    // /// Returns the dot product of p1 and p2.
    // pub fn dot_product(p1: &QLineF, p2: &QLineF) -> f64 {
    //     ffi::qlinef_dot_product(p1, p2)
    // }

    /// Constructs a point with the given coordinates (x, y).
    pub fn new(x: f64, y: f64) -> Self {
        ffi::qlinef_init(x, y)
    }
}

impl Default for QLineF {
    /// Constructs a null point, i.e. with coordinates (0.0, 0.0)
    fn default() -> Self {
        ffi::qlinef_init_default()
    }
}

impl fmt::Display for QLineF {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", ffi::qlinef_to_qstring(self))
    }
}

impl From<&ffi::QPoint> for QLineF {
    /// Constructs a copy of the given point.
    fn from(point: &ffi::QPoint) -> Self {
        ffi::qlinef_from_qpoint(point)
    }
}

impl From<QLineF> for ffi::QPoint {
    /// Rounds the coordinates of this point to the nearest integer,
    /// and returns a QPoint object with the rounded coordinates.
    fn from(value: QLineF) -> Self {
        value.to_point()
    }
}

impl std::ops::Add for QLineF {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        ffi::qlinef_plus(&self, &other)
    }
}

impl std::ops::Sub for QLineF {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        ffi::qlinef_minus(&self, &other)
    }
}

impl std::ops::Mul<f64> for QLineF {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        ffi::qlinef_mul(rhs, &self)
    }
}

impl std::ops::Div<f64> for QLineF {
    type Output = Self;
    fn div(self, rhs: f64) -> Self {
        ffi::qlinef_div(rhs, &self)
    }
}

// Safety:
//
// Static checks on the C++ side ensure that QLineF is trivial.
unsafe impl ExternType for QLineF {
    type Id = type_id!("QLineF");
    type Kind = cxx::kind::Trivial;
}

impl QListElement for QLineF {}