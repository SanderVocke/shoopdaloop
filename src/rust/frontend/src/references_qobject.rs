use anyhow::anyhow;
use cxx_qt_lib::QVariant;
use cxx_qt_lib_shoop::{
    qobject::QObject,
    qpointer::{
        qpointer_from_qobject, qpointer_to_qobject, qvariant_from_qpointer, QPointerQObject,
    },
    qsharedpointer_qobject::QSharedPointer_QObject,
    qvariant_helpers::{
        qobject_ptr_to_qvariant, qsharedpointer_qobject_to_qvariant, qvariant_to_qobject_ptr,
        qvariant_to_qsharedpointer_qobject, qvariant_to_qweakpointer_qobject,
        qweakpointer_qobject_to_qvariant,
    },
    qweakpointer_qobject::QWeakPointer_QObject,
};

pub trait ReferencesQObject: Sized {
    fn as_qobject_ptr(&mut self) -> *mut QObject;

    fn as_qobject_ref(&self) -> *const QObject;

    fn copy(&self) -> Self;

    fn to_qvariant(&self) -> Result<QVariant, anyhow::Error>;

    fn from_qvariant(qvariant: &QVariant) -> Result<Self, anyhow::Error>;

    fn points_to_same_object(&self, other: &Self) -> bool {
        self.as_qobject_ref() == other.as_qobject_ref()
    }
}

impl ReferencesQObject for *mut QObject {
    fn as_qobject_ptr(&mut self) -> *mut QObject {
        *self as *mut QObject
    }

    fn as_qobject_ref(&self) -> *const QObject {
        *self as *const QObject
    }

    fn copy(&self) -> Self {
        self.clone()
    }

    fn to_qvariant(&self) -> Result<QVariant, anyhow::Error> {
        Ok(qobject_ptr_to_qvariant(self)?)
    }

    fn from_qvariant(qvariant: &QVariant) -> Result<Self, anyhow::Error> {
        Ok(qvariant_to_qobject_ptr(qvariant)?)
    }
}

impl ReferencesQObject for cxx::UniquePtr<QPointerQObject> {
    fn as_qobject_ptr(&mut self) -> *mut QObject {
        self.as_ref()
            .map(|pointer| unsafe { qpointer_to_qobject(pointer) })
            .unwrap_or(std::ptr::null_mut())
    }

    fn as_qobject_ref(&self) -> *const QObject {
        self.as_ref()
            .map(|pointer| unsafe { qpointer_to_qobject(pointer) } as *const QObject)
            .unwrap_or(std::ptr::null())
    }

    fn copy(&self) -> Self {
        self.as_ref()
            .map(|pointer| unsafe { qpointer_from_qobject(qpointer_to_qobject(pointer)) })
            .unwrap_or_else(cxx::UniquePtr::null)
    }

    fn to_qvariant(&self) -> Result<QVariant, anyhow::Error> {
        self.as_ref()
            .map(|pointer| unsafe { qvariant_from_qpointer(pointer) })
            .ok_or_else(|| anyhow!("empty QPointer"))
    }

    fn from_qvariant(qvariant: &QVariant) -> Result<Self, anyhow::Error> {
        let object = qvariant_to_qobject_ptr(qvariant)?;
        if object.is_null() {
            return Err(anyhow!("null QObject"));
        }
        Ok(unsafe { qpointer_from_qobject(object) })
    }
}

impl ReferencesQObject for cxx::UniquePtr<QSharedPointer_QObject> {
    fn as_qobject_ptr(&mut self) -> *mut QObject {
        self.data().unwrap_or(std::ptr::null_mut())
    }

    fn as_qobject_ref(&self) -> *const QObject {
        self.data().unwrap_or(std::ptr::null_mut()) as *const QObject
    }

    fn copy(&self) -> Self {
        match self.as_ref() {
            Some(obj) => QSharedPointer_QObject::copy(obj).unwrap_or(cxx::UniquePtr::null()),
            None => cxx::UniquePtr::null(),
        }
    }

    fn to_qvariant(&self) -> Result<QVariant, anyhow::Error> {
        Ok(qsharedpointer_qobject_to_qvariant(
            self.as_ref().ok_or(anyhow!("empty unique ptr"))?,
        )?)
    }

    fn from_qvariant(qvariant: &QVariant) -> Result<Self, anyhow::Error> {
        Ok(qvariant_to_qsharedpointer_qobject(qvariant)?)
    }
}

impl ReferencesQObject for cxx::UniquePtr<QWeakPointer_QObject> {
    fn as_qobject_ptr(&mut self) -> *mut QObject {
        match self.as_ref() {
            Some(obj) => match obj.to_strong() {
                Ok(Some(obj)) => obj.data().unwrap_or(std::ptr::null_mut()),
                _ => std::ptr::null_mut(),
            },
            None => std::ptr::null_mut(),
        }
    }

    fn as_qobject_ref(&self) -> *const QObject {
        match self.as_ref() {
            Some(obj) => match obj.to_strong() {
                Ok(Some(obj)) => obj.data().unwrap_or(std::ptr::null_mut()) as *const QObject,
                _ => std::ptr::null(),
            },
            None => std::ptr::null(),
        }
    }

    fn copy(&self) -> Self {
        match self.as_ref() {
            Some(obj) => obj.copy().unwrap_or(cxx::UniquePtr::null()),
            None => cxx::UniquePtr::null(),
        }
    }

    fn to_qvariant(&self) -> Result<QVariant, anyhow::Error> {
        Ok(qweakpointer_qobject_to_qvariant(
            self.as_ref().ok_or(anyhow!("empty unique ptr"))?,
        )?)
    }

    fn from_qvariant(qvariant: &QVariant) -> Result<Self, anyhow::Error> {
        Ok(qvariant_to_qweakpointer_qobject(qvariant)?)
    }
}
