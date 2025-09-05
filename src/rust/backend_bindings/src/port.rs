use crate::ffi;
use enum_iterator::*;
use num_enum::{IntoPrimitive, TryFromPrimitive};

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum PortDirection {
    Input = ffi::shoop_port_direction_t_ShoopPortDirection_Input,
    Output = ffi::shoop_port_direction_t_ShoopPortDirection_Output,
    Any = ffi::shoop_port_direction_t_ShoopPortDirection_Any,
}

pub struct ExternalPortDescriptor {
    pub name: String,
    pub direction: PortDirection,
    pub data_type: PortDataType,
}

impl ExternalPortDescriptor {
    pub fn new(ffi_descriptor: &ffi::shoop_external_port_descriptor_t) -> Self {
        ExternalPortDescriptor {
            name: unsafe {
                std::ffi::CStr::from_ptr(ffi_descriptor.name)
                    .to_str()
                    .unwrap()
                    .to_string()
            },
            direction: PortDirection::try_from(ffi_descriptor.direction).unwrap(),
            data_type: PortDataType::try_from(ffi_descriptor.data_type).unwrap(),
        }
    }

    pub fn to_ffi(&self) -> ffi::shoop_external_port_descriptor_t {
        let c_name = std::ffi::CString::new(self.name.clone()).unwrap();
        ffi::shoop_external_port_descriptor_t {
            name: c_name.into_raw(),
            direction: self.direction as ffi::shoop_port_direction_t,
            data_type: self.data_type as ffi::shoop_port_data_type_t,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum PortDataType {
    Audio = ffi::shoop_port_data_type_t_ShoopPortDataType_Audio,
    Midi = ffi::shoop_port_data_type_t_ShoopPortDataType_Midi,
    Any = ffi::shoop_port_data_type_t_ShoopPortDataType_Any,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum PortConnectabilityKind {
    None = 0,
    Internal = ffi::shoop_port_connectability_t_ShoopPortConnectability_Internal,
    External = ffi::shoop_port_connectability_t_ShoopPortConnectability_External,
}

impl PortConnectabilityKind {
    pub fn from_ffi(ffi_kind: u32) -> Self {
        const INTERNAL: u32 =
            ffi::shoop_port_connectability_t_ShoopPortConnectability_Internal as u32;
        const EXTERNAL: u32 =
            ffi::shoop_port_connectability_t_ShoopPortConnectability_External as u32;
        match ffi_kind {
            0 => PortConnectabilityKind::None,
            INTERNAL => PortConnectabilityKind::Internal,
            EXTERNAL => PortConnectabilityKind::External,
            _ => panic!("Invalid PortConnectabilityKind value: {}", ffi_kind),
        }
    }

    pub fn to_ffi(&self) -> u32 {
        *self as u32
    }
}

#[derive(Debug, PartialEq, Clone, Default)]
pub struct PortConnectability {
    pub internal: bool,
    pub external: bool,
}

impl PortConnectability {
    pub fn from_ffi(ffi_connectability: u32) -> Self {
        PortConnectability {
            internal: ffi_connectability
                & (ffi::shoop_port_connectability_t_ShoopPortConnectability_Internal as u32)
                != 0,
            external: ffi_connectability
                & (ffi::shoop_port_connectability_t_ShoopPortConnectability_External as u32)
                != 0,
        }
    }

    pub fn to_ffi(&self) -> u32 {
        let mut ffi_connectability: u32 = 0;
        if self.internal {
            ffi_connectability |=
                ffi::shoop_port_connectability_t_ShoopPortConnectability_Internal as u32;
        }
        if self.external {
            ffi_connectability |=
                ffi::shoop_port_connectability_t_ShoopPortConnectability_External as u32;
        }
        ffi_connectability
    }
}
