use crate::cxx_qt_lib_shoop::invokable;
use cxx_qt_lib::QString;
use cxx::Exception;

pub mod constants {
    pub const INVOKABLE_DETERMINE_CONNECTIONS_STATE: &str = "determine_connections_state()";
    pub const INVOKABLE_BOOL_CONNECT_EXTERNAL_PORT: &str = "connect_external_port(QString)";

    pub const PROP_DATA_TYPE: &str = "data_type";
    pub const SIGNAL_DATA_TYPE_CHANGED: &str = "data_typeChanged()";

    pub const PROP_DIRECTION: &str = "direction";
    pub const SIGNAL_DIRECTION_CHANGED: &str = "directionChanged()";

    pub const PROP_INITIALIZED: &str = "initialized";
    pub const SIGNAL_INITIALIZED_CHANGED: &str = "initializedChanged()";

    pub const PROP_NAME: &str = "name";
    pub const SIGNAL_NAME_CHANGED: &str = "nameChanged()";
}

pub fn invoke_connect_external_port<T>(obj: &mut T, connection_type : u32, port_name: QString) -> Result<bool, Exception>
where 
    T : invokable::QObjectOrConvertible,
{
    let args = port_name;
    invokable::invoke(obj, constants::INVOKABLE_BOOL_CONNECT_EXTERNAL_PORT.to_string(), connection_type, &args)
}

pub fn invoke_determine_connections_state<T>(obj: &mut T, connection_type : u32) -> Result<cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>, Exception>
where 
    T : invokable::QObjectOrConvertible,
{
    invokable::invoke(obj, constants::INVOKABLE_DETERMINE_CONNECTIONS_STATE.to_string(), connection_type, &())
}