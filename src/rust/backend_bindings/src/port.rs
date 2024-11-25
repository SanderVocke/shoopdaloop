use crate::integer_enum;
use crate::ffi;

integer_enum! {
    pub enum PortDirection {
        Input = ffi::shoop_port_direction_t_ShoopPortDirection_Input,
        Output = ffi::shoop_port_direction_t_ShoopPortDirection_Output,
        Any = ffi::shoop_port_direction_t_ShoopPortDirection_Any,
    }
}

integer_enum! {
    pub enum PortDataType {
        Audio = ffi::shoop_port_data_type_t_ShoopPortDataType_Audio,
        Midi = ffi::shoop_port_data_type_t_ShoopPortDataType_Midi,
        Any = ffi::shoop_port_data_type_t_ShoopPortDataType_Any,
    }
}

integer_enum! {
    pub enum PortConnectability {
        Internal = ffi::shoop_port_connectability_t_ShoopPortConnectability_Internal,
        External = ffi::shoop_port_connectability_t_ShoopPortConnectability_External,
    }
}