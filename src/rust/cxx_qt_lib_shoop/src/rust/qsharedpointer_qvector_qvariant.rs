use cxx::UniquePtr;

#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qvector.h");
        include!("cxx-qt-lib/qvariant.h");
        include!("cxx-qt-lib-shoop/qsharedpointer_qvector_qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        type QVector_QVariant = cxx_qt_lib::QVector<QVariant>;
        type QSharedPointer_QVector_QVariant;

        #[rust_name = "qsharedpointer_qvector_qvariant_from_ptr"]
        unsafe fn qSharedPointerQVectorQVariantFromPtr(
            ptr: *mut QVector_QVariant,
        ) -> Result<UniquePtr<QSharedPointer_QVector_QVariant>>;

        #[rust_name = "qsharedpointer_qvector_qvariant_copy"]
        unsafe fn qSharedPointerCopy(
            other: &QSharedPointer_QVector_QVariant,
        ) -> Result<UniquePtr<QSharedPointer_QVector_QVariant>>;

        #[rust_name = "qsharedpointer_qvector_qvariant_data"]
        unsafe fn qSharedPointerData(
            ptr: &QSharedPointer_QVector_QVariant,
        ) -> Result<*mut QVector_QVariant>;
    }
}

pub use ffi::QSharedPointer_QVector_QVariant;

impl QSharedPointer_QVector_QVariant {
    pub fn from_ptr(
        ptr: *mut ffi::QVector_QVariant,
    ) -> Result<UniquePtr<QSharedPointer_QVector_QVariant>, cxx::Exception> {
        unsafe { ffi::qsharedpointer_qvector_qvariant_from_ptr(ptr) }
    }

    pub fn copy(&self) -> Result<UniquePtr<QSharedPointer_QVector_QVariant>, cxx::Exception> {
        unsafe { ffi::qsharedpointer_qvector_qvariant_copy(self) }
    }

    pub fn data(&self) -> Result<*mut ffi::QVector_QVariant, cxx::Exception> {
        unsafe { ffi::qsharedpointer_qvector_qvariant_data(self) }
    }
}

unsafe impl Send for QSharedPointer_QVector_QVariant {}
unsafe impl Sync for QSharedPointer_QVector_QVariant {}
