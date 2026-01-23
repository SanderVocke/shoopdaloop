use crate::ffi;
use common::logging::macros::*;
use enum_iterator::*;

shoop_log_unit!("BackendBindings.Port");
use num_enum::{IntoPrimitive, TryFromPrimitive};

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum PortDirection {
    Input = ffi::shoop_port_direction_t_ShoopPortDirection_Input as i32,
    Output = ffi::shoop_port_direction_t_ShoopPortDirection_Output as i32,
    Any = ffi::shoop_port_direction_t_ShoopPortDirection_Any as i32,
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
                    .to_string_lossy()
                    .into_owned()
            },
            direction: PortDirection::try_from(ffi_descriptor.direction as i32)
                .unwrap_or(PortDirection::Any),
            data_type: PortDataType::try_from(ffi_descriptor.data_type as i32)
                .unwrap_or(PortDataType::Any),
        }
    }

    pub fn to_ffi(&self) -> ffi::shoop_external_port_descriptor_t {
        let c_name = match std::ffi::CString::new(self.name.clone()) {
            Ok(c) => c,
            Err(e) => {
                error!("Invalid CString for port name: {}", e);
                std::ffi::CString::default()
            }
        };
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
    Audio = ffi::shoop_port_data_type_t_ShoopPortDataType_Audio as i32,
    Midi = ffi::shoop_port_data_type_t_ShoopPortDataType_Midi as i32,
    Any = ffi::shoop_port_data_type_t_ShoopPortDataType_Any as i32,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum PortConnectabilityKind {
    None = 0,
    Internal = ffi::shoop_port_connectability_t_ShoopPortConnectability_Internal as i32,
    External = ffi::shoop_port_connectability_t_ShoopPortConnectability_External as i32,
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
            _ => {
                error!("Invalid PortConnectabilityKind value: {}", ffi_kind);
                PortConnectabilityKind::None
            }
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
