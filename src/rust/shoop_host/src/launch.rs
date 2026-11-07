use crate::{config::HostConfig, connection, remote::Target, serve::StartOptions};
use anyhow::{ensure, Context, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use shoop_app_protocol::{ManagementReply, ManagementRequest, VERSION};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};

#[derive(Deserialize, Serialize)]
pub struct Startup {
    pub config: HostConfig,
    pub options: StartOptions,
}

pub fn read_message<T: DeserializeOwned>(mut reader: impl Read) -> Result<T> {
    let mut length = [0; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_le_bytes(length) as usize;
    ensure!(length > 0 && length <= 65536, "invalid launch message size");
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}
pub fn write_message(mut writer: impl Write, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= 65536,
        "launch message exceeds 64 KiB"
    );
    writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()?;
    Ok(())
}
pub fn companion_path(application: &Path) -> Result<PathBuf> {
    named_companion_path(application, "shoop_host")
}
pub fn looper_path(application: &Path) -> Result<PathBuf> {
    named_companion_path(application, "shoopdaloop")
}
pub fn manager_path(application: &Path) -> Result<PathBuf> {
    named_companion_path(application, "shoop_host_manager")
}
fn named_companion_path(application: &Path, name: &str) -> Result<PathBuf> {
    let application = application
        .canonicalize()
        .unwrap_or_else(|_| application.to_owned());
    Ok(application
        .parent()
        .context("application has no parent directory")?
        .join(if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.to_owned()
        }))
}
pub fn launch_manager() -> Result<()> {
    let mut command = Command::new(manager_path(&std::env::current_exe()?)?);
    independent(&mut command);
    reap(command.spawn().context("Open Host Manager")?);
    Ok(())
}
pub fn independent(command: &mut Command) {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x00000200 | 0x00000008);
    }
}
pub fn reap(mut child: Child) {
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}
pub fn launch_ui(command: &mut Command, management: &str) -> Result<()> {
    ensure!(
        management.len() <= 320,
        "Host endpoint exceeds its size limit"
    );
    let mut bytes = Vec::new();
    write_message(&mut bytes, &management)?;
    independent(command);
    command.stdin(Stdio::piped());
    let mut child = command.spawn().context("Start independent UI")?;
    let result = child
        .stdin
        .take()
        .context("UI startup pipe unavailable")
        .and_then(|mut input| input.write_all(&bytes).context("Pass host endpoint to UI"));
    reap(child);
    result
}
pub fn spawn(
    config: &HostConfig,
    options: StartOptions,
    host_executable: &Path,
) -> Result<(Child, Target)> {
    config.validate()?;
    let mut command = Command::new(host_executable);
    command.arg("--launch-stdin");
    independent(&mut command);
    command.stdin(Stdio::piped()).stdout(Stdio::piped());
    let mut child = command.spawn().context("start independent session host")?;
    let result = (|| -> Result<Target> {
        let stdin = child
            .stdin
            .take()
            .context("host startup pipe unavailable")?;
        let stdout = child
            .stdout
            .take()
            .context("host readiness pipe unavailable")?;
        let startup = Startup {
            config: config.clone(),
            options,
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let result = (|| {
                write_message(stdin, &startup)?;
                read_message::<Target>(stdout)
            })();
            let _ = sender.send(result);
        });
        let mut target = receiver
            .recv_timeout(Duration::from_secs(12))
            .context("host readiness timed out")??;
        ensure!(
            target.process == shoop_host_processes::identity(child.id())?
                && target.process.is_some()
                && target.instance != 0,
            "Host readiness process identity mismatch"
        );
        target.use_local_addresses();
        ensure!(
            matches!(
                connection::management(
                    &target,
                    ManagementRequest::Status { version: VERSION },
                    Duration::from_secs(2)
                ),
                Ok(ManagementReply::Status { ready: true, .. })
            ),
            "host did not confirm readiness"
        );
        Ok(target)
    })();
    match result {
        Ok(target) => Ok((child, target)),
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(error)
        }
    }
}
pub fn pair(
    config: &HostConfig,
    options: StartOptions,
    host_executable: &Path,
    ui: impl FnOnce(&Target) -> Command,
) -> Result<Target> {
    let (child, target) = spawn(config, options, host_executable)?;
    reap(child);
    let mut command = ui(&target);
    launch_ui(&mut command, &target.management).with_context(|| {
        format!(
            "UI failed to start; ready host remains available at {}",
            target.management
        )
    })?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[shoop_wasm_test_support::shoop_test]
    fn application_bundle_aliases_find_the_real_companions() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().canonicalize().unwrap();
        let shared = directory.join("ShoopDaLoop.app/Contents/MacOS");
        let entry = directory.join("ShoopDaLoop Host Manager.app/Contents/MacOS");
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::create_dir_all(&entry).unwrap();
        let manager = shared.join("shoop_host_manager");
        std::fs::write(&manager, b"fixture executable").unwrap();
        let alias = entry.join("shoop_host_manager");
        std::os::unix::fs::symlink(
            "../../../ShoopDaLoop.app/Contents/MacOS/shoop_host_manager",
            &alias,
        )
        .unwrap();
        assert_eq!(
            super::companion_path(&alias).unwrap(),
            shared.join("shoop_host")
        );
        assert_eq!(
            super::looper_path(&alias).unwrap(),
            shared.join("shoopdaloop")
        );
        assert_eq!(super::manager_path(&alias).unwrap(), manager);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn launch_frames_are_bounded_and_reject_partial_messages() {
        let mut bytes = Vec::new();
        super::write_message(&mut bytes, &vec![1_u32, 2, 3]).unwrap();
        assert_eq!(
            super::read_message::<Vec<u32>>(&bytes[..]).unwrap(),
            [1, 2, 3]
        );
        assert!(super::read_message::<Vec<u32>>(&bytes[..bytes.len() - 1]).is_err());
        assert!(super::read_message::<Vec<u32>>(&65537_u32.to_le_bytes()[..]).is_err());
        assert!(super::write_message(Vec::new(), &"x".repeat(65537)).is_err());
    }
}
