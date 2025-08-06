use common::logging::macros::*;
shoop_log_unit!("Frontend.TestPort");

pub use crate::cxx_qt_shoop::test::qobj_test_port_bridge::ffi::make_unique_test_port as make_unique;
pub use crate::cxx_qt_shoop::test::qobj_test_port_bridge::ffi::TestPort;
use crate::cxx_qt_shoop::test::qobj_test_port_bridge::ffi::*;
use cxx::UniquePtr;
use cxx_qt::CxxQtType;
use std::pin::Pin;

impl TestPort {
    pub fn set_connections_state_from_json(self: Pin<&mut Self>, json: &str) -> Result<(), String> {
        let json_obj: UniquePtr<QJsonObject> = QJsonObject::from_json(json)?;
        let map: QMap_QString_QVariant = json_obj.as_ref().unwrap().to_variant_map()?;
        self.set_connections_state(map);
        Ok(())
    }

    pub fn determine_connections_state(self: Pin<&mut TestPort>) -> QMap_QString_QVariant {
        self.connections_state().clone().into()
    }

    pub fn connect_external_port(mut self: Pin<&mut TestPort>, name: QString) -> bool {
        let rval: bool = self.as_mut().rust_mut().connect_external_port_return_val;
        self.as_mut().external_connection_made(name);
        return rval;
    }
}
