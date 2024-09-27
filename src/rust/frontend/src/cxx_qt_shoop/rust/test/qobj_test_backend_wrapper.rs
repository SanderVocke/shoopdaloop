use common::logging::macros::*;
shoop_log_unit!("Frontend.TestBackendWrapper");

pub use crate::cxx_qt_shoop::test::qobj_test_backend_wrapper_bridge::TestBackendWrapper;
pub use crate::cxx_qt_shoop::test::qobj_test_backend_wrapper_bridge::constants::*;
pub use crate::cxx_qt_shoop::test::qobj_test_backend_wrapper_bridge::ffi::make_unique_test_backend_wrapper as make_unique;
use crate::cxx_qt_shoop::test::qobj_test_backend_wrapper_bridge::ffi::*;
use crate::cxx_qt_lib_shoop::qobject::AsQObject;
use std::pin::Pin;
use crate::cxx_qt_lib_shoop::qobject;
use crate::cxx_qt_shoop::fn_qlist_helpers;
use crate::cxx_qt_shoop::type_external_port_descriptor::ExternalPortDescriptor;
use cxx_qt::CxxQtType;

impl TestBackendWrapper {
    unsafe fn initialize_impl_with_result(self : Pin<&mut TestBackendWrapper>) -> Result<(), cxx::Exception> {
        qobject::qobject_set_object_name(self.pin_mut_qobject_ptr(), String::from("shoop_backend_wrapper"))?;
        Ok(())
    }

    pub fn initialize_impl(self : Pin<&mut TestBackendWrapper>) {
        unsafe {
            match self.initialize_impl_with_result() {
                Ok(()) => (),
                Err(e) => { error!("Unable to initialize: {:?}", e); }
            }
        }
    }

    pub fn find_external_ports(self: Pin<&mut Self>,
                               _maybe_regex : QString,
                               _port_direction : i32,
                               _data_type : i32) -> QList_QVariant {
        let rust = self.rust_mut();
        return match fn_qlist_helpers::as_qlist_qvariant::<ExternalPortDescriptor>
            (&rust.mock_external_ports)
        {
            Ok(list) => list,
            Err(_) => {
                error!("Could not convert mock_external_ports to QList<QVariant>");
                QList_QVariant::default()
            }
        }
    }
}