//! Bounded private logs for independent native hosts.
use crate::private;
use anyhow::Result;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

const LIMIT: u64 = 1024 * 1024;
const BACKUPS: usize = 3;

fn backup(path: &Path, index: usize) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(format!(".{index}"));
    PathBuf::from(name)
}

struct Log {
    path: PathBuf,
    file: Option<File>,
    length: u64,
}
impl Log {
    fn open(path: PathBuf) -> Result<Self> {
        if !path.exists() {
            private::create(&path, &[])?;
        }
        private::check(&path, false)?;
        let file = OpenOptions::new().append(true).open(&path)?;
        let length = file.metadata()?.len();
        Ok(Self {
            path,
            file: Some(file),
            length,
        })
    }
    fn rotate(&mut self) -> io::Result<()> {
        self.file.take();
        for index in (1..=BACKUPS).rev() {
            let destination = backup(&self.path, index);
            let source = if index == 1 {
                self.path.clone()
            } else {
                backup(&self.path, index - 1)
            };
            if destination.exists() {
                private::check(&destination, false).map_err(io::Error::other)?;
                fs::remove_file(&destination)?;
            }
            if source.exists() {
                private::check(&source, false).map_err(io::Error::other)?;
                fs::rename(source, destination)?;
            }
        }
        *self = Self::open(self.path.clone()).map_err(io::Error::other)?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct Writer(Arc<Mutex<Log>>);
impl Writer {
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self(Arc::new(Mutex::new(Log::open(path.to_owned())?))))
    }
}
impl Write for Writer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let _ = io::stderr().write_all(bytes);
        let mut log = self
            .0
            .lock()
            .map_err(|_| io::Error::other("host log lock poisoned"))?;
        for chunk in bytes.chunks(LIMIT as usize) {
            if log.length + chunk.len() as u64 > LIMIT {
                log.rotate()?;
            }
            log.file
                .as_mut()
                .ok_or_else(|| io::Error::other("host log unavailable"))?
                .write_all(chunk)?;
            log.length += chunk.len() as u64;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        let mut log = self
            .0
            .lock()
            .map_err(|_| io::Error::other("host log lock poisoned"))?;
        log.file
            .as_mut()
            .ok_or_else(|| io::Error::other("host log unavailable"))?
            .flush()
    }
}

/// Read only the latest bounded tail; logs survive host shutdown.
pub fn tail(path: &Path) -> Result<String> {
    private::check(path, false)?;
    let mut file = File::open(path)?;
    let start = file.metadata()?.len().saturating_sub(64 * 1024);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.take(64 * 1024).read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    #[shoop_wasm_test_support::shoop_test]
    fn rotates_private_logs_and_bounds_the_tail() {
        use super::*;
        let root = tempfile::tempdir().unwrap();
        let mut writer = Writer::open(&root.path().join("host.log")).unwrap();
        // Bypass the stderr mirror for the large rotation fixture.
        let mut log = writer.0.lock().unwrap();
        log.file
            .as_mut()
            .unwrap()
            .write_all(&vec![b'a'; LIMIT as usize])
            .unwrap();
        log.length = LIMIT;
        drop(log);
        writer.write_all(b"new host diagnostic\n").unwrap();
        let path = root.path().join("host.log");
        assert_eq!(tail(&path).unwrap(), "new host diagnostic\n");
        let backup = path.with_extension("log.1");
        assert_eq!(fs::metadata(&backup).unwrap().len(), LIMIT);
        assert_eq!(tail(&backup).unwrap().len(), 64 * 1024);
        private::check(&path, false).unwrap();
        for _ in 0..5 {
            writer.0.lock().unwrap().rotate().unwrap();
        }
        assert!(!path.with_extension("log.4").exists());
    }
}
