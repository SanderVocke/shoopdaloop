//! Rust-native bridge-object helpers.
//!
//! This is the Rust-side counterpart of the C++ `BridgeStrong<T>` /
//! `BridgeWeak<T>` helpers. It wraps Rust-native `Arc<T>` / `Weak<T>` smart
//! pointers in concrete, CXX-opaque per-type wrapper structs so C++ code can
//! own, clone, downgrade, upgrade and use Rust-owned objects while a type is
//! being migrated from C++ to Rust.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

static NEXT_RUST_BRIDGE_OBJECT_ID: AtomicU64 = AtomicU64::new(1);

fn next_bridge_object_id() -> u64 {
    NEXT_RUST_BRIDGE_OBJECT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Strong bridge handle for a Rust-native object.
///
/// This owns one `Arc<T>` reference when valid. Invalid strong handles are used
/// as the non-nullable return value of failed `Weak::upgrade()` calls across the
/// CXX bridge, because `rust::Box<T>` cannot represent null.
pub struct RustBridgeStrong<T> {
    id: u64,
    inner: Option<Arc<T>>,
}

/// Weak bridge handle for a Rust-native object.
///
/// This owns one `Weak<T>` reference and can be upgraded to a
/// `RustBridgeStrong<T>` while the object is still alive.
pub struct RustBridgeWeak<T> {
    id: u64,
    inner: Weak<T>,
}

impl<T> RustBridgeStrong<T> {
    /// Create a new strong bridge handle around a freshly allocated object.
    pub fn new(inner: T) -> Self {
        Self::from_arc(Arc::new(inner))
    }

    /// Create a new strong bridge handle around an existing `Arc<T>`.
    pub fn from_arc(inner: Arc<T>) -> Self {
        Self::from_arc_with_id(next_bridge_object_id(), inner)
    }

    /// Create a new strong bridge handle with an explicit stable id.
    pub fn from_arc_with_id(id: u64, inner: Arc<T>) -> Self {
        Self {
            id,
            inner: Some(inner),
        }
    }

    /// Create an invalid strong handle. Used for failed weak upgrades.
    pub fn invalid(id: u64) -> Self {
        Self { id, inner: None }
    }

    /// Stable id shared by all bridge handles derived from the same root handle.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Whether this handle owns a live `Arc<T>`.
    pub fn valid(&self) -> bool {
        self.inner.is_some()
    }

    /// Create a weak bridge handle for the same object.
    pub fn downgrade(&self) -> RustBridgeWeak<T> {
        RustBridgeWeak {
            id: self.id,
            inner: self.inner.as_ref().map(Arc::downgrade).unwrap_or_default(),
        }
    }

    /// Clone the strong bridge handle, incrementing the `Arc<T>` strong count.
    pub fn clone_strong(&self) -> RustBridgeStrong<T> {
        RustBridgeStrong {
            id: self.id,
            inner: self.inner.clone(),
        }
    }

    /// Borrow the inner object if this handle is valid.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> Option<R> {
        self.inner.as_ref().map(|inner| f(inner.as_ref()))
    }

    /// Get a cloned `Arc<T>` if this handle is valid.
    pub fn arc(&self) -> Option<Arc<T>> {
        self.inner.clone()
    }

    /// Current `Arc<T>` strong count, or zero for invalid handles.
    pub fn strong_count(&self) -> usize {
        self.inner.as_ref().map(Arc::strong_count).unwrap_or(0)
    }

    /// Current `Arc<T>` weak count, or zero for invalid handles.
    pub fn weak_count(&self) -> usize {
        self.inner.as_ref().map(Arc::weak_count).unwrap_or(0)
    }
}

impl<T> Clone for RustBridgeStrong<T> {
    fn clone(&self) -> Self {
        self.clone_strong()
    }
}

impl<T> RustBridgeWeak<T> {
    /// Stable id shared by all bridge handles derived from the same root handle.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Whether this weak handle can currently be upgraded.
    pub fn valid(&self) -> bool {
        self.inner.strong_count() > 0
    }

    /// Upgrade to a strong bridge handle. Returns an invalid strong handle if
    /// the object has expired.
    pub fn upgrade(&self) -> RustBridgeStrong<T> {
        match self.inner.upgrade() {
            Some(inner) => RustBridgeStrong::from_arc_with_id(self.id, inner),
            None => RustBridgeStrong::invalid(self.id),
        }
    }

    /// Clone this weak bridge handle.
    pub fn clone_weak(&self) -> RustBridgeWeak<T> {
        RustBridgeWeak {
            id: self.id,
            inner: self.inner.clone(),
        }
    }

    /// Temporarily upgrade and borrow the inner object if it is still alive.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> Option<R> {
        self.inner.upgrade().map(|inner| f(inner.as_ref()))
    }

    /// Current `Arc<T>` strong count.
    pub fn strong_count(&self) -> usize {
        self.inner.strong_count()
    }

    /// Current `Arc<T>` weak count.
    pub fn weak_count(&self) -> usize {
        self.inner.weak_count()
    }
}

impl<T> Clone for RustBridgeWeak<T> {
    fn clone(&self) -> Self {
        self.clone_weak()
    }
}

/// Define concrete CXX-exposable bridge wrapper structs for a Rust-native type.
///
/// CXX cannot expose generic Rust types, so each bridged Rust-native object type
/// should define concrete wrappers with this macro and then list those concrete
/// types and methods in its `#[cxx::bridge]` module.
#[macro_export]
macro_rules! define_rust_bridge_object_wrappers {
    ($strong:ident, $weak:ident, $target:ty) => {
        pub struct $strong(pub(crate) $crate::rust_bridge_object::RustBridgeStrong<$target>);

        pub struct $weak(pub(crate) $crate::rust_bridge_object::RustBridgeWeak<$target>);

        impl $strong {
            pub(crate) fn from_arc(inner: std::sync::Arc<$target>) -> Self {
                Self($crate::rust_bridge_object::RustBridgeStrong::from_arc(
                    inner,
                ))
            }

            pub fn id(&self) -> u64 {
                self.0.id()
            }

            pub fn valid(&self) -> bool {
                self.0.valid()
            }

            pub fn downgrade(&self) -> Box<$weak> {
                Box::new($weak(self.0.downgrade()))
            }

            pub fn clone_strong(&self) -> Box<$strong> {
                Box::new($strong(self.0.clone_strong()))
            }

            pub fn strong_count(&self) -> usize {
                self.0.strong_count()
            }

            pub fn weak_count(&self) -> usize {
                self.0.weak_count()
            }
        }

        impl $weak {
            pub fn id(&self) -> u64 {
                self.0.id()
            }

            pub fn valid(&self) -> bool {
                self.0.valid()
            }

            pub fn upgrade(&self) -> Box<$strong> {
                Box::new($strong(self.0.upgrade()))
            }

            pub fn clone(&self) -> Box<$weak> {
                Box::new($weak(self.0.clone_weak()))
            }

            pub fn strong_count(&self) -> usize {
                self.0.strong_count()
            }

            pub fn weak_count(&self) -> usize {
                self.0.weak_count()
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct DropCounter {
        drops: Arc<AtomicUsize>,
    }

    impl Drop for DropCounter {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn strong_downgrades_and_weak_upgrades() {
        let strong = RustBridgeStrong::new(123_u32);
        let weak = strong.downgrade();

        assert!(strong.valid());
        assert!(weak.valid());
        assert_eq!(strong.id(), weak.id());

        let upgraded = weak.upgrade();
        assert!(upgraded.valid());
        assert_eq!(upgraded.id(), strong.id());
        assert_eq!(upgraded.with(|value| *value), Some(123));
    }

    #[test]
    fn weak_expires_when_strong_handles_are_gone() {
        let drops = Arc::new(AtomicUsize::new(0));
        let weak = {
            let strong = RustBridgeStrong::new(DropCounter {
                drops: drops.clone(),
            });
            let weak = strong.downgrade();
            assert!(weak.valid());
            weak
        };

        assert_eq!(drops.load(Ordering::SeqCst), 1);
        assert!(!weak.valid());

        let upgraded = weak.upgrade();
        assert!(!upgraded.valid());
        assert_eq!(upgraded.id(), weak.id());
    }

    #[test]
    fn cloned_strong_keeps_object_alive() {
        let drops = Arc::new(AtomicUsize::new(0));
        let strong = RustBridgeStrong::new(DropCounter {
            drops: drops.clone(),
        });
        let weak = strong.downgrade();
        let strong_clone = strong.clone_strong();

        drop(strong);
        assert!(weak.valid());
        assert_eq!(drops.load(Ordering::SeqCst), 0);

        drop(strong_clone);
        assert!(!weak.valid());
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn weak_with_temporarily_upgrades_without_returning_a_strong_wrapper() {
        let strong = RustBridgeStrong::new(7_u32);
        let weak = strong.downgrade();

        assert_eq!(weak.with(|value| *value + 5), Some(12));
        drop(strong);
        assert_eq!(weak.with(|value| *value + 5), None);
    }
}
