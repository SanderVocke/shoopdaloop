use cxx::UniquePtr;

#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qvector.h");
        include!("cxx-qt-lib-shoop/qsharedpointer_qvector_f32.h");
        type QVector_f32 = cxx_qt_lib::QVector<f32>;
        type QSharedPointer_QVector_f32;

        #[rust_name = "qsharedpointer_qvector_f32_from_ptr"]
        unsafe fn qSharedPointerQVectorf32FromPtr(
            ptr: *mut QVector_f32,
        ) -> Result<UniquePtr<QSharedPointer_QVector_f32>>;

        #[rust_name = "qsharedpointer_qvector_f32_copy"]
        unsafe fn qSharedPointerCopy(
            other: &QSharedPointer_QVector_f32,
        ) -> Result<UniquePtr<QSharedPointer_QVector_f32>>;

        #[rust_name = "qsharedpointer_qvector_f32_data"]
        unsafe fn qSharedPointerData(ptr: &QSharedPointer_QVector_f32) -> Result<*mut QVector_f32>;
    }
}

pub use ffi::QSharedPointer_QVector_f32;

impl QSharedPointer_QVector_f32 {
    pub fn from_ptr(
        ptr: *mut ffi::QVector_f32,
    ) -> Result<UniquePtr<QSharedPointer_QVector_f32>, cxx::Exception> {
        unsafe { ffi::qsharedpointer_qvector_f32_from_ptr(ptr) }
    }

    pub fn copy(&self) -> Result<UniquePtr<QSharedPointer_QVector_f32>, cxx::Exception> {
        unsafe { ffi::qsharedpointer_qvector_f32_copy(self) }
    }

    pub fn data(&self) -> Result<*mut ffi::QVector_f32, cxx::Exception> {
        unsafe { ffi::qsharedpointer_qvector_f32_data(self) }
    }
}

unsafe impl Send for QSharedPointer_QVector_f32 {}
unsafe impl Sync for QSharedPointer_QVector_f32 {}
