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
    info!("init_crashhandling called: is_server={is_server}, start_server_arg={start_server_arg}, pid={}", std::process::id());

    let maybe_dump_folder: Option<String> = std::env::var("SHOOP_CRASHDUMP_DIR").ok();
    if let Some(dump_folder) = maybe_dump_folder.as_ref() {
        debug!("Using dump folder: {:?}", dump_folder);
        let path = std::path::PathBuf::from(dump_folder);
        if !path.exists() || !path.is_dir() {
            warn!("Dump folder {path:?} does not exist or is not a directory - crash handler not enabled.");
            return;
        }
    } else {
        debug!("Dumps will go to OS-specific temp folder");
    }

    if is_server {
        debug!("Entering server mode");
        crashhandling_server();

        // Unreachable
        return;
    }

    debug!("Entering client mode");
    let handle = crashhandling_client(start_server_arg, on_crash_callback);
    let _ =
        CLIENTSIDE_HANDLE.get_or_init(|| -> Option<client::CrashHandlerHandle> { Some(handle) });
    debug!("Client handle stored in CLIENTSIDE_HANDLE, thread is running");
}

pub fn set_crash_json_partial(partial_json: JsonValue) {
    let handle = CLIENTSIDE_HANDLE.get_or_init(|| None);
    let handle = if let Some(h) = handle.as_ref() {
        h
    } else {
        debug!("set_metadata called, but no crash handling client active");
        return;
    };
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
