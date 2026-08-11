#[cfg(all(not(target_arch = "wasm32"), feature = "native-store"))]
mod native_store;
mod settings;

#[cfg(all(not(target_arch = "wasm32"), feature = "native-store"))]
pub use native_store::*;
pub use settings::*;

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CarlaHostingMode {
    #[default]
    InProcess,
    Subprocess,
}

impl CarlaHostingMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InProcess => "in_process",
            Self::Subprocess => "subprocess",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CarlaHostingModeParseError {
    value: String,
}

impl fmt::Display for CarlaHostingModeParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "unsupported Carla hosting mode {:?}; expected in_process or subprocess",
            self.value
        )
    }
}

impl std::error::Error for CarlaHostingModeParseError {}

impl FromStr for CarlaHostingMode {
    type Err = CarlaHostingModeParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "in_process" => Ok(Self::InProcess),
            "subprocess" => Ok(Self::Subprocess),
            _ => Err(CarlaHostingModeParseError {
                value: value.to_owned(),
            }),
        }
    }
}

impl TryFrom<&str> for CarlaHostingMode {
    type Error = CarlaHostingModeParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carla_hosting_modes_have_stable_validated_strings() {
        for mode in [CarlaHostingMode::InProcess, CarlaHostingMode::Subprocess] {
            assert_eq!(mode.as_str().parse::<CarlaHostingMode>().unwrap(), mode);
        }
        assert_eq!(CarlaHostingMode::default(), CarlaHostingMode::InProcess);
        assert!("external".parse::<CarlaHostingMode>().is_err());
    }
}
