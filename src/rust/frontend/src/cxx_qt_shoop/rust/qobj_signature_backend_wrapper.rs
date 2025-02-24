use std::pin::Pin;
use cxx::Exception;
use cxx_qt_lib::{QList, QVariant, QString};

use crate::cxx_qt_lib_shoop::invokable;
use crate::cxx_qt_lib_shoop::qobject::{QObject, AsQObject};

pub mod constants {
    pub const PROP_READY: &str = "ready";

    pub const SIGNAL_UPDATED_ON_GUI_THREAD: &str = "updated_on_gui_thread()";
    pub const SIGNAL_UPDATED_ON_BACKEND_THREAD : &str = "updated_on_backend_thread()";
    
    pub const INVOKABLE_UPDATE_ON_OTHER_THREAD: &str = "update_on_other_thread()";
    pub const INVOKABLE_FIND_EXTERNAL_PORTS: &str = "find_external_ports(QString,int,int)";
}

pub fn invoke_find_external_ports<T>(obj: &mut T, port_name: QString, port_direction: i32, port_type: i32) -> Result<QList<QVariant>, Exception>
where 
    T : AsQObject,
{
    let args = (port_name, port_direction, port_type);
    invokable::invoke(obj, constants::INVOKABLE_FIND_EXTERNAL_PORTS.to_string(), &args)
}