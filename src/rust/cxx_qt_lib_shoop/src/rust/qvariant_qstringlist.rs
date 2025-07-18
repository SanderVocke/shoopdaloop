#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qvariant.h");
        include!("cxx-qt-lib/qlist.h");
        include!("cxx-qt-lib/qstring.h");
        type QVariant = cxx_qt_lib::QVariant;
        type QString = cxx_qt_lib::QString;
        type QList_QString = cxx_qt_lib::QList<QString>;

        include!("cxx-qt-lib-shoop/qvariant_qstringlist.h");

        #[rust_name = "qvariant_as_qstringlist"]
        fn qvariantAsQStringList(v: &QVariant) -> Result<QList_QString>;

        #[rust_name = "qstringlist_as_qvariant"]
        fn qstringlistAsQvariant(v: &QList_QString) -> Result<QVariant>;

        #[rust_name = "qvariant_convertible_to_qstringlist"]
        fn qvariantConvertibleToQStringlist(v: &QVariant) -> Result<bool>;
    }
}

pub use ffi::{
    qstringlist_as_qvariant, qvariant_as_qstringlist, qvariant_convertible_to_qstringlist,
    QList_QString,
};
