pub mod constants {
    pub const PROP_READY: &str = "ready";

    pub const SIGNAL_UPDATED_ON_GUI_THREAD: &str = "updated_on_gui_thread()";
    pub const SIGNAL_UPDATED_ON_BACKEND_THREAD : &str = "updated_on_backend_thread()";
    
    pub const INVOKABLE_UPDATE_ON_OTHER_THREAD: &str = "update_on_other_thread()";
    pub const INVOKABLE_FIND_EXTERNAL_PORTS: &str = "find_external_ports(QString,int,int)";
}