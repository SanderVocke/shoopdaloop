#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qvariant.h");
        include!("cxx-qt-lib/qlist.h");
        type QVariant = cxx_qt_lib::QVariant;
        type QList_QVariant = cxx_qt_lib::QList<QVariant>;

        include!("cxx-qt-lib-shoop/qvariant_qvariantlist.h");

        #[rust_name = "qvariant_as_qvariantlist"]
        fn qvariantAsQvariantList(v: &QVariant) -> Result<QList_QVariant>;

        #[rust_name = "qvariantlist_as_qvariant"]
        fn qvariantlistAsQvariant(v: &QList_QVariant) -> Result<QVariant>;

        #[rust_name = "qvariant_convertible_to_qvariantlist"]
        fn qvariantConvertibleToQvariantlist(v: &QVariant) -> Result<bool>;
    }
}

pub use ffi::{
    qvariant_as_qvariantlist, qvariant_convertible_to_qvariantlist, qvariantlist_as_qvariant,
    QList_QVariant,
};
