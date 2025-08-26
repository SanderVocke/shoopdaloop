// Note: mostly taken from the example included with the minidumper crate.

pub mod registered_threads;

use serde_json::Value as JsonValue;
use std::sync::OnceLock;

mod client;
mod server;
mod types;

use client::crashhandling_client;
use server::crashhandling_server;
pub use types::*;

use common::logging::macros::*;
shoop_log_unit!("CrashHandling");

static CLIENTSIDE_HANDLE: OnceLock<Option<client::CrashHandlerHandle>> = OnceLock::new();

pub fn init_crashhandling(
    is_server: bool,
    start_server_arg: &str,
    on_crash_callback: Option<client::CrashCallback>,
) {
    let maybe_dump_folder: Option<String> = std::env::var("SHOOP_CRASHDUMP_DIR").ok();
    if maybe_dump_folder.is_some() {
        debug!(
            "Using dump folder: {:?}",
            maybe_dump_folder.as_ref().unwrap()
        );
        let path = std::path::PathBuf::from(maybe_dump_folder.unwrap());
        if !path.exists() || !path.is_dir() {
            warn!("Dump folder {path:?} does not exist or is not a directory - crash handler not enabled.");
            return;
        }
    } else {
        debug!("Dumps will go to OS-specific temp folder");
    }

    if is_server {
        crashhandling_server();

        // Unreachable
        return;
    }

    let handle = crashhandling_client(start_server_arg, on_crash_callback);
    let _ =
        CLIENTSIDE_HANDLE.get_or_init(|| -> Option<client::CrashHandlerHandle> { Some(handle) });
}

pub fn set_crash_json_partial(partial_json: JsonValue) {
    let handle = CLIENTSIDE_HANDLE.get_or_init(|| None);
    if handle.is_none() {
        debug!("set_metadata called, but no crash handling client active");
        return;
    }

    let handle = handle.as_ref().unwrap();
    let content = partial_json.to_string();
    match handle.sender.send(CrashHandlingMessage {
        message_type: CrashHandlingMessageType::SetJson,
        message: content,
    }) {
        Ok(_) => (),
        Err(e) => error!("Failed to send crash handling message: {}", e),
    };
}

pub fn set_crash_json_toplevel_field(key: &str, value: JsonValue) {
    set_crash_json_partial(serde_json::json!({key: value}));
}

pub fn set_crash_json_tag(key: &str, value: JsonValue) {
    set_crash_json_partial(serde_json::json!({"tags": {key: value}}));
}

pub fn set_crash_json_extra(key: &str, value: JsonValue) {
    set_crash_json_partial(serde_json::json!({"extra": {key: value}}));
}
