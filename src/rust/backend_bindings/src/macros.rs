#[macro_export]
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

        impl std::convert::TryFrom<u32> for $name {
            type Error = anyhow::Error;

            fn try_from(v: u32) -> Result<Self, Self::Error> {
                match v {
                    $(x if x == $name::$vname as u32 => Ok($name::$vname),)*
                    _ => Err(anyhow::anyhow!("Enum value out of range")),
                }
            }
        }

        impl PartialEq for $name {
            fn eq(&self, other : &$name) -> bool { *self as i32 == *other as i32 }
        }

        impl core::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                match self {
                    $(x if x == &$name::$vname => write!(f, stringify!($vname)),)*
                    _ => write!(f, "Unknown"),
                }
            }
        }
    }
}
