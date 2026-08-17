use std::cell::Cell;
use std::rc::Rc;

use anyhow::anyhow;
use omnilua::{Function, Lua, Value};
use shoop_app_api::{LuaApiVersion, LUA_API_VERSION};

use crate::{install_compatibility_value, runtime_error};

pub const ANNOUNCE_API_VERSION_FUNCTION: &str = "shoop_announce_api_version";

#[derive(Clone, Copy, Default)]
enum AnnouncementStatus {
    #[default]
    Missing,
    Accepted(LuaApiVersion),
    Incompatible(LuaApiVersion),
    Rejected,
}

#[derive(Default)]
pub struct ApiVersionState {
    status: Cell<AnnouncementStatus>,
    stop_after_announcement: Cell<bool>,
}

impl ApiVersionState {
    pub fn require_announced(&self) -> omnilua::Result<LuaApiVersion> {
        match self.status.get() {
            AnnouncementStatus::Accepted(version) => Ok(version),
            AnnouncementStatus::Missing => Err(runtime_error(format!(
                "{ANNOUNCE_API_VERSION_FUNCTION}({}, {}) must be the first Shoop API call",
                LUA_API_VERSION.major, LUA_API_VERSION.minor
            ))),
            AnnouncementStatus::Incompatible(_) | AnnouncementStatus::Rejected => Err(
                runtime_error("Shoop Lua API version announcement was rejected"),
            ),
        }
    }

    fn announce(&self, requested: LuaApiVersion) -> omnilua::Result<()> {
        match self.status.get() {
            AnnouncementStatus::Accepted(previous) => {
                self.status.set(AnnouncementStatus::Rejected);
                return Err(runtime_error(format!(
                    "{ANNOUNCE_API_VERSION_FUNCTION} may only be called once (already announced {}.{})",
                    previous.major, previous.minor
                )));
            }
            AnnouncementStatus::Incompatible(_) | AnnouncementStatus::Rejected => {
                return Err(runtime_error(
                    "Shoop Lua API version announcement was already rejected",
                ));
            }
            AnnouncementStatus::Missing => {}
        }
        if !LUA_API_VERSION.accepts(requested) {
            self.status.set(AnnouncementStatus::Incompatible(requested));
            return Err(runtime_error(format!(
                "incompatible Shoop Lua API: script requests {}.{}, host supports {}.{}",
                requested.major, requested.minor, LUA_API_VERSION.major, LUA_API_VERSION.minor
            )));
        }
        self.status.set(AnnouncementStatus::Accepted(requested));
        if self.stop_after_announcement.get() {
            Err(runtime_error("Lua API compatibility probe complete"))
        } else {
            Ok(())
        }
    }

    pub fn incompatible_version(&self) -> Option<LuaApiVersion> {
        match self.status.get() {
            AnnouncementStatus::Incompatible(version) => Some(version),
            _ => None,
        }
    }

    pub fn stop_after_announcement(&self) {
        self.stop_after_announcement.set(true);
    }

    fn reject(&self, message: String) -> omnilua::Error {
        self.status.set(AnnouncementStatus::Rejected);
        runtime_error(message)
    }
}

pub fn install_api_version_announcement(
    lua: &Lua,
    run_sandboxed: &Function,
    state: Rc<ApiVersionState>,
) -> anyhow::Result<()> {
    let announce = lua
        .create_function(move |_, (major, minor): (Value, Value)| {
            let major =
                version_component(major, "major").map_err(|message| state.reject(message))?;
            let minor =
                version_component(minor, "minor").map_err(|message| state.reject(message))?;
            state.announce(LuaApiVersion { major, minor })
        })
        .map_err(|error| anyhow!("could not create Lua API announcement: {error}"))?;
    install_compatibility_value(run_sandboxed, ANNOUNCE_API_VERSION_FUNCTION, announce)
}

fn version_component(value: Value, name: &str) -> Result<u32, String> {
    let Value::Integer(value) = value else {
        return Err(format!(
            "Lua API {name} version must be a non-negative integer"
        ));
    };
    u32::try_from(value)
        .map_err(|_| format!("Lua API {name} version must be a non-negative integer"))
}
