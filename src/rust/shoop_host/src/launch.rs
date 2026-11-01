use crate::{
    config::HostConfig,
    connection, private,
    registry::{HostEntry, Registry},
    serve::StartOptions,
};
use anyhow::{anyhow, ensure, Context, Result};
use shoop_app_protocol::{ManagementReply, ManagementRequest, VERSION};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub fn companion_path(application: &Path) -> Result<PathBuf> {
    Ok(application
        .parent()
        .context("application has no parent directory")?
        .join(if cfg!(windows) {
            "shoop_host.exe"
        } else {
            "shoop_host"
        }))
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
pub fn pair(
    config: &HostConfig,
    options: StartOptions,
    host_executable: &Path,
    ui: impl FnOnce(&Path) -> Command,
) -> Result<PathBuf> {
    config.validate()?;
    let registry = Registry::open(&options.registry)?;
    let path = registry.entry_path(options.host);
    let config_path = registry
        .root()
        .join(format!("{:032x}.launch", options.host.0));
    private::create(&config_path, &serde_json::to_vec(config)?)?;
    let mut command = Command::new(host_executable);
    command
        .arg("--config")
        .arg(&config_path)
        .arg("--registry")
        .arg(registry.root())
        .arg("--endpoint")
        .arg(options.endpoint.to_string())
        .arg("--host-id")
        .arg(options.host.0.to_string())
        .arg("--session-id")
        .arg(options.session.0.to_string())
        .arg("--name")
        .arg(&options.name);
    if let Some(credential) = options.credential {
        command.arg("--credential-file").arg(credential);
    }
    independent(&mut command);
    let mut child = command.spawn().context("start independent session host")?;
    let deadline = Instant::now() + Duration::from_secs(12);
    let ready = loop {
        if path.exists() {
            let entry = HostEntry::read(&path)?;
            ensure!(
                entry.host == options.host && entry.session == options.session,
                "launched host identity differs from readiness entry"
            );
            if matches!(
                connection::management(
                    &entry,
                    ManagementRequest::Status {
                        version: VERSION,
                        host: entry.host,
                        session: entry.session
                    },
                    Duration::from_millis(500)
                ),
                Ok(ManagementReply::Status { ready: true, .. })
            ) {
                break Ok(());
            }
        }
        if let Some(status) = child.try_wait()? {
            break Err(anyhow!("session host failed during startup ({status})"));
        }
        if Instant::now() >= deadline {
            break Err(anyhow!(
                "session host readiness timed out; registry: {}",
                path.display()
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let _ = std::fs::remove_file(config_path);
    if let Err(error) = ready {
        // A published host survives launcher failure; rollback is limited to our unready launch.
        if !path.exists() {
            let _ = child.kill();
            let _ = child.wait();
            let credential = registry.credential_path(options.host);
            if credential.exists() {
                let _ = std::fs::remove_file(credential);
            }
        }
        return Err(error);
    }
    let mut command = ui(&path);
    independent(&mut command);
    command.spawn().with_context(|| {
        format!(
            "UI failed to start; ready host remains available at {}",
            path.display()
        )
    })?;
    Ok(path)
}
