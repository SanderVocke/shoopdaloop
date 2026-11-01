use anyhow::{ensure, Context, Result};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

pub fn directory(path: &Path) -> Result<()> {
    if !path.exists() {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(path)
            .context("create protected session registry")?;
        #[cfg(windows)]
        acl(path, true)?;
    }
    check(path, true)
}

pub fn check(path: &Path, directory: bool) -> Result<()> {
    let metadata = fs::symlink_metadata(path).context("inspect protected session path")?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "protected session paths cannot be symlinks"
    );
    ensure!(
        if directory {
            metadata.is_dir()
        } else {
            metadata.is_file()
        },
        "unexpected protected session path type"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        ensure!(
            metadata.uid() == nix::unistd::geteuid().as_raw(),
            "session path belongs to another user"
        );
        ensure!(
            metadata.permissions().mode() & 0o077 == 0,
            "session path must deny group and other access"
        );
    }
    #[cfg(windows)]
    acl(path, false)?;
    Ok(())
}

pub fn create(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .context("create protected session file")?;
    #[cfg(windows)]
    acl(path, true)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    check(path, false)
}

pub fn read(path: &Path, limit: usize) -> Result<Vec<u8>> {
    check(path, false)?;
    let file = File::open(path)?;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= limit,
        "protected session file exceeds its size limit"
    );
    Ok(bytes)
}

#[cfg(windows)]
fn acl(path: &Path, protect: bool) -> Result<()> {
    use std::process::{Command, Stdio};
    const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$p = $env:SHOOP_PRIVATE_PATH
$me = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
$a = Get-Acl -LiteralPath $p
if ($env:SHOOP_PRIVATE_PROTECT -eq '1') {
    $a.SetAccessRuleProtection($true, $false)
    foreach ($r in @($a.Access)) { [void]$a.RemoveAccessRuleAll($r) }
    $a.SetOwner($me)
    $inherit = [System.Security.AccessControl.InheritanceFlags]::None
    if ((Get-Item -LiteralPath $p).PSIsContainer) { $inherit = [System.Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit' }
    $rule = [System.Security.AccessControl.FileSystemAccessRule]::new($me, [System.Security.AccessControl.FileSystemRights]::FullControl, $inherit, [System.Security.AccessControl.PropagationFlags]::None, [System.Security.AccessControl.AccessControlType]::Allow)
    [void]$a.AddAccessRule($rule)
    Set-Acl -LiteralPath $p -AclObject $a
    $a = Get-Acl -LiteralPath $p
}
if ($a.GetOwner([System.Security.Principal.SecurityIdentifier]).Value -ne $me.Value) { throw 'session path belongs to another user' }
foreach ($r in $a.GetAccessRules($true, $true, [System.Security.Principal.SecurityIdentifier])) {
    if ($r.AccessControlType -eq [System.Security.AccessControl.AccessControlType]::Allow -and $r.IdentityReference.Value -notin @($me.Value, 'S-1-5-18', 'S-1-5-32-544')) { throw 'session path permits another user' }
}
"#;
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .env("SHOOP_PRIVATE_PATH", path)
        .env("SHOOP_PRIVATE_PROTECT", if protect { "1" } else { "0" })
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .context("run session ACL adapter")?;
    ensure!(
        output.status.success(),
        "session path ACL validation failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[shoop_wasm_test_support::shoop_test]
    fn credential_files_are_private_bounded_and_cannot_follow_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("credential");
        create(&path, &[7; 32]).unwrap();
        assert_eq!(read(&path, 32).unwrap(), [7; 32]);
        assert!(read(&path, 31).is_err());
        assert!(create(&path, &[8; 32]).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let link = root.path().join("link");
            std::os::unix::fs::symlink(&path, &link).unwrap();
            assert!(read(&link, 32).is_err());
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(read(&path, 32).is_err());
        }
    }
}
