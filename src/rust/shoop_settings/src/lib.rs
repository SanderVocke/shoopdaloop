mod egui_settings;
#[cfg(all(not(target_arch = "wasm32"), feature = "legacy"))]
mod legacy_settings;
#[cfg(all(not(target_arch = "wasm32"), feature = "native-store"))]
mod native_store;

pub use egui_settings::*;
#[cfg(all(not(target_arch = "wasm32"), feature = "legacy"))]
pub use legacy_settings::*;
#[cfg(all(not(target_arch = "wasm32"), feature = "native-store"))]
pub use native_store::*;

use serde::{Deserialize, Serialize};

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
