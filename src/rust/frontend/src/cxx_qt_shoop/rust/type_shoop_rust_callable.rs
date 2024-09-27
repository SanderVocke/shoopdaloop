#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
    }

    unsafe extern "C++" {
        include!("cxx-qt-shoop/shoop_rust_callable.h");
        type ShoopRustCallable = super::ShoopRustCallableRust;

        #[rust_name = "qvariant_can_convert_shoop_rust_callable"]
        fn qvariantCanConvertShoopRustCallable(variant: &QVariant) -> bool;

        #[rust_name = "register_metatype_shoop_rust_callable"]
        fn registerMetatypeShoopRustCallable(name : &mut String);
    }

    #[namespace = "rust::cxxqtlib1::qvariant"]
    unsafe extern "C++" {
        include!("cxx-qt-lib/qvariant.h");

        #[rust_name = "qvariant_construct_shoop_rust_callable"]
        fn qvariantConstruct(value: &ShoopRustCallable) -> QVariant;

        #[rust_name = "qvariant_value_or_default_shoop_rust_callable"]
        fn qvariantValueOrDefault(variant: &QVariant) -> ShoopRustCallable;
    }
}

pub use ffi::ShoopRustCallable;
use ffi::*;

#[repr(C)]
pub struct ShoopRustCallableRust {
    callable : fn(&[QVariant]) -> QVariant,
}

impl Default for ShoopRustCallableRust {
    fn default() -> Self {
        Self {
            callable : |_a : &[QVariant]| -> QVariant {
                QVariant::default()
            }
        }
    }
}

unsafe impl cxx::ExternType for ShoopRustCallable {
    type Id = cxx::type_id!("ShoopRustCallable");
    type Kind = cxx::kind::Trivial;
}

impl ShoopRustCallable {
    pub fn call(self : &ShoopRustCallable, args : Option<&[QVariant]>) -> QVariant {
        let rust : &ShoopRustCallableRust = self.into();
        (rust.callable)(args.unwrap_or(&[]))
    }
}

impl cxx_qt_lib::QVariantValue for ffi::ShoopRustCallable {
    fn can_convert(variant: &cxx_qt_lib::QVariant) -> bool {
        ffi::qvariant_can_convert_shoop_rust_callable(variant)
    }

    fn construct(value: &Self) -> cxx_qt_lib::QVariant {
        ffi::qvariant_construct_shoop_rust_callable(value)
    }

    fn value_or_default(variant: &cxx_qt_lib::QVariant) -> Self {
        ffi::qvariant_value_or_default_shoop_rust_callable(variant)
    }
}

pub fn register_metatype(name : &str) {
    let mut str = String::from(name);
    ffi::register_metatype_shoop_rust_callable(&mut str);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_callable_no_data() {
        let mut obj = ShoopRustCallableRust::default();
        obj.callable = |_v : &[QVariant]| -> QVariant {
            let rval = 123;
            QVariant::from(&rval)
        };
        let output : QVariant = obj.call(None);
        let output2 : QVariant = obj.call(None);

        assert_eq!(output.value::<i32>().unwrap(), 123);
        assert_eq!(output2.value::<i32>().unwrap(), 123);
    }
}