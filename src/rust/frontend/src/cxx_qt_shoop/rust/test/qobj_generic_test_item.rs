use common::logging::macros::*;
shoop_log_unit!("Frontend.GenericTestItem");

use crate::cxx_qt_shoop::test::qobj_generic_test_item_bridge::ffi::make_unique_generic_test_item;
pub use crate::cxx_qt_shoop::test::qobj_generic_test_item_bridge::GenericTestItem;

impl GenericTestItem {
    pub fn add(self: &GenericTestItem, a: i32, b: i32) -> i32 {
        a + b
    }

    pub fn make_unique() -> cxx::UniquePtr<GenericTestItem> {
        make_unique_generic_test_item()
    }
}
