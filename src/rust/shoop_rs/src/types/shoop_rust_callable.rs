#[repr(C)]
pub struct ShoopRustCallable {}

unsafe impl cxx::ExternType for ShoopRustCallable {
    type Id = cxx::type_id!("ShoopRustCallable");
    type Kind = cxx::kind::Trivial;
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

#[cxx_qt::bridge(cxx_file_stem = "shoop_rust_callable")]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
    }

    unsafe extern "C++" {
        include!("cxx-shoop/shoop_rust_callable.h");
        type ShoopRustCallable = super::ShoopRustCallable;

        #[rust_name = "qvariant_can_convert_shoop_rust_callable"]
        fn qvariantCanConvertShoopRustCallable(variant: &QVariant) -> bool;

        #[rust_name = "register_metatype_shoop_rust_callable"]
        fn registerMetatypeShoopRustCallable(name : &mut String);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qvariant.h");

        #[rust_name = "qvariant_construct_shoop_rust_callable"]
        fn qvariantConstruct(value: &ShoopRustCallable) -> QVariant;

        #[rust_name = "qvariant_value_or_default_shoop_rust_callable"]
        fn qvariantValueOrDefault(variant: &QVariant) -> ShoopRustCallable;
    }
}

pub fn register_metatype(name : &str) {
    let mut str = String::from(name);
    ffi::register_metatype_shoop_rust_callable(&mut str);
}