mod raw {
    #![allow(non_snake_case)]
    #![allow(non_camel_case_types)]
    #![allow(non_upper_case_globals)]
    #![allow(unused)]
    include!("codegen/libshoopdaloop.rs");
}

use core::fmt::Debug;

macro_rules! integer_enum {
    ($(#[$meta:meta])* $vis:vis enum $name:ident {
        $($(#[$vmeta:meta])* $vname:ident $(= $val:expr)?,)*
    }) => {
        $(#[$meta])*
        #[derive(Copy, Clone)]
        $vis enum $name {
            $($(#[$vmeta])* $vname $(= $val as isize)?,)*
        }

        impl std::convert::TryFrom<i32> for $name {
            type Error = anyhow::Error;

            fn try_from(v: i32) -> Result<Self, Self::Error> {
                match v {
                    $(x if x == $name::$vname as i32 => Ok($name::$vname),)*
                    _ => Err(anyhow::anyhow!("Enum value out of range")),
                }
            }
        }

        impl PartialEq for $name {
            fn eq(&self, other : &$name) -> bool { *self as i32 == *other as i32 }
        }

        impl Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                match self {
                    $(x if x == &$name::$vname => write!(f, stringify!($vname)),)*
                    _ => write!(f, "Unknown"),
                }
            }
        }
    }
}

integer_enum! {
pub enum PortDirection {
    Input = raw::shoop_port_direction_t_ShoopPortDirection_Input,
    Output = raw::shoop_port_direction_t_ShoopPortDirection_Output,
    Any = raw::shoop_port_direction_t_ShoopPortDirection_Any,
}
}

integer_enum! {
pub enum PortDataType {
    Audio = raw::shoop_port_data_type_t_ShoopPortDataType_Audio,
    Midi = raw::shoop_port_data_type_t_ShoopPortDataType_Midi,
    Any = raw::shoop_port_data_type_t_ShoopPortDataType_Any,
}
}