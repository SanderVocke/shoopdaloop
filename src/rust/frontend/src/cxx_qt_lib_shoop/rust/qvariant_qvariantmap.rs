#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qvariant.h");
        include!("cxx-qt-lib/qmap.h");
        type QVariant = cxx_qt_lib::QVariant;
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;

        include!("cxx-qt-lib-shoop/qvariant_qvariantmap.h");

        #[rust_name = "qvariant_as_qvariantmap"]
        fn qvariantAsQvariantMap(v: &QVariant) -> Result<QMap_QString_QVariant>;

        #[rust_name = "qvariantmap_as_qvariant"]
        fn qvariantmapAsQvariant(v: &QMap_QString_QVariant) -> Result<QVariant>;

        #[rust_name = "qvariant_convertible_to_qvariantmap"]
        fn qvariantConvertibleToQvariantmap(v: &QVariant) -> Result<bool>;
    }
}

pub use ffi::{
    qvariant_as_qvariantmap, qvariant_convertible_to_qvariantmap, qvariantmap_as_qvariant,
};
