use crate::types::*;
use common::logging::macros::*;
use minidumper::Server;
use serde_json::Value as JsonValue;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
shoop_log_unit!("CrashHandlingServer");

fn set_json(to: &mut JsonValue, from: &JsonValue) -> anyhow::Result<()> {
    if from.is_object() {
        if !to.is_object() {
            return Err(anyhow::anyhow!("expected object"));
        }

        for (k, v) in from.as_object().unwrap().iter() {
            if v.is_object() {
                if to.get(k).is_none() {
                    to[k] = serde_json::json!({});
                } else if !to.get(k).unwrap().is_object() {
                    return Err(anyhow::anyhow!("expected object"));
                }
                return set_json(&mut to[k], v);
            } else if v.is_array() {
                if to.get(k).is_none() {
                    to[k] = serde_json::json!([]);
                } else if !to.get(k).unwrap().is_array() {
                    return Err(anyhow::anyhow!("expected array"));
                }
                return set_json(&mut to[k], v);
            } else {
                to[k] = v.clone();
            }
        }
    } else {
        *to = from.clone();
    }
    Ok(())
}

pub fn crashhandling_server() {
    debug!("Starting crash handling server");

    let socket_name = std::env::args().last().expect("no socket name provided");

    debug!("Server: crash handling socket: {socket_name}");

    let mut server =
        Server::with_name(socket_name.as_str()).expect("failed to create crash handling server");

    let ab = std::sync::atomic::AtomicBool::new(false);
    struct Handler {
        json: Mutex<JsonValue>,
        last_written_dump: Mutex<Option<PathBuf>>, // For interior mutability
    }

    impl minidumper::ServerHandler for Handler {
        /// Called when a crash has been received and a backing file needs to be
        /// created to store it.
        fn create_minidump_file(
            &self,
        ) -> Result<(std::fs::File, std::path::PathBuf), std::io::Error> {
            let maybe_dump_folder: Option<String> = std::env::var("SHOOP_CRASHDUMP_DIR").ok();

            let (dumpfile, dumpfilepath) = (if maybe_dump_folder.is_some() {
                let path = std::path::PathBuf::from(maybe_dump_folder.as_ref().unwrap());
                tempfile::NamedTempFile::with_suffix_in(".dmp", &path)
            } else {
                tempfile::NamedTempFile::with_suffix(".dmp")
            })
            .expect("Could not open temp file for minidump")
            .into_parts();
            let dumpfilepath = dumpfilepath.keep().expect("Could not persist temp file");
            let mut ref_last_written_dump = self.last_written_dump.lock().unwrap();
            *ref_last_written_dump = Some(dumpfilepath.clone());

            info!("Writing minidump to {}", dumpfilepath.display());
            Ok((dumpfile, dumpfilepath))
        }

        /// Called when a crash has been fully written as a minidump to the provided
        /// file. Also returns the full heap buffer as well.
        fn on_minidump_created(
            &self,
            result: Result<minidumper::MinidumpBinary, minidumper::Error>,
        ) -> minidumper::LoopAction {
            match result {
                Ok(mut md_bin) => {
                    use std::io::Write;
                    let _ = md_bin.file.flush();
                    info!("Done writing minidump");

                    debug!("Writing crash metadata json...");
                    let path = &md_bin.path;

                    let result = || -> Result<(), anyhow::Error> {
                        let filename = path
                            .file_name()
                            .ok_or(anyhow::anyhow!("Could not get dump filename"))?
                            .to_str()
                            .ok_or(anyhow::anyhow!("Could not convert dump filename"))?;
                        let new_filename = format!("{filename}.metadata.json");
                        let mut json_path: std::path::PathBuf = path.clone();
                        json_path.set_file_name(new_filename);

                        // Write the metadata json to this file
                        let guard = self
                            .json
                            .lock()
                            .map_err(|e| anyhow::anyhow!("Failed to lock json: {e:?}"))?;
                        let content = serde_json::to_string_pretty(&*guard).map_err(|e| {
                            anyhow::anyhow!("Failed to serialize metadata json: {e:?}")
                        })?;
                        let content = format!("{content}\n");
                        let mut jsonfile = File::create(&json_path)?;
                        jsonfile.write_all(content.as_bytes())?;
                        jsonfile.flush()?;
                        info!("Wrote crash metadata json to {json_path:?}");

                        Ok(())
                    }();
                    match result {
                        Ok(()) => (),
                        Err(e) => error!("Failed to write crash metadata json: {e:?}"),
                    };
                }
                Err(e) => {
                    error!("Failed to write minidump: {:#}", e);
                }
            }

            // Tells the server to continue
            minidumper::LoopAction::Continue
        }

        fn on_message(&self, kind: u32, buffer: Vec<u8>) {
            let content = String::from_utf8(buffer).unwrap();
            match kind {
                v if v == CrashHandlingMessageType::SetJson as usize as u32 => {
                    let result = || -> anyhow::Result<()> {
                        debug!("Set JSON: {content:?}");
                        let mut guard = self
                            .json
                            .lock()
                            .map_err(|e| anyhow::anyhow!("Could not lock json: {e:?}"))?;
                        let incoming_json: JsonValue = serde_json::from_str(content.as_str())?;
                        set_json(&mut *guard, &incoming_json)?;
                        Ok(())
                    }();
                    match result {
                        Ok(()) => (),
                        Err(e) => error!("Failed to update crash metadata json: {e:?}"),
                    };
                }
                v if v == CrashHandlingMessageType::AdditionalCrashAttachment as usize as u32 => {
                    let attachment: AdditionalCrashAttachment =
                        serde_json::from_str(content.as_str()).unwrap();
                    let id = attachment.id;
                    let ref_last_written_dump = self.last_written_dump.lock().unwrap();
                    let last_written_dump = &*ref_last_written_dump;
                    if last_written_dump.is_none() {
                        warn!("Received additional crash data attachment ('{id}'), but no dump written. Ignoring.");
                    } else {
                        let result = || -> anyhow::Result<()> {
                            let path = last_written_dump
                                .as_ref()
                                .ok_or(anyhow::anyhow!("No dump file remembered"))?;
                            let filename = path.file_name().unwrap().to_string_lossy();
                            let new_filename = format!("{filename}.attach.{id}");
                            let mut attach_path: std::path::PathBuf = path.clone();
                            attach_path.set_file_name(new_filename);
                            let mut file = File::create(&attach_path)?;
                            file.write_all(attachment.contents.as_bytes())?;
                            file.flush()?;
                            info!("Wrote crash attachment '{id}' to {attach_path:?}");
                            Ok(())
                        }();
                        match result {
                            Ok(()) => (),
                            Err(e) => error!("Failed to write crash attachment '{id}': {e:?}"),
                        }
                    }
                }
                _ => error!("Unknown message kind {kind}, content {content}"),
            };
        }

        fn on_client_disconnected(&self, _num_clients: usize) -> minidumper::LoopAction {
            debug!("Client disconnected, # -> {_num_clients}");
            if _num_clients == 0 {
                minidumper::LoopAction::Exit
            } else {
                minidumper::LoopAction::Continue
            }
        }

        fn on_client_connected(&self, _num_clients: usize) -> minidumper::LoopAction {
            debug!("Client connected, # -> {_num_clients}");
            minidumper::LoopAction::Continue
        }
    }

    let maybe_base_json: Option<String> = std::env::var("SHOOP_CRASH_METADATA_BASE_JSON").ok();
    let base_json: JsonValue = if maybe_base_json.is_some() {
        serde_json::from_str(maybe_base_json.unwrap().as_str())
            .expect("invalid base crash handling json")
    } else {
        serde_json::json!({ "environment": "unknown" })
    };

    server
        .run(
            Box::new(Handler {
                json: Mutex::new(base_json),
                last_written_dump: Mutex::new(None),
            }),
            &ab,
            Some(std::time::Duration::from_millis(5000)),
        )
        .expect("failed to run server");

    info!("Crash handling server exiting");

    std::process::exit(0);
}
