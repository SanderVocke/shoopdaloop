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

pub struct PortConnectability {
    pub internal : bool,
    pub external : bool,
}

impl PortConnectability {
    pub fn from_ffi(ffi_connectability : u32) -> Self {
        PortConnectability {
            internal : ffi_connectability & ffi::shoop_port_connectability_t_ShoopPortConnectability_Internal != 0,
            external : ffi_connectability & ffi::shoop_port_connectability_t_ShoopPortConnectability_External != 0,
        }
    }

    pub fn to_ffi(&self) -> u32 {
        let mut ffi_connectability = 0;
        if self.internal {
            ffi_connectability |= ffi::shoop_port_connectability_t_ShoopPortConnectability_Internal;
        }
        if self.external {
            ffi_connectability |= ffi::shoop_port_connectability_t_ShoopPortConnectability_External;
        }
        ffi_connectability
    }
}