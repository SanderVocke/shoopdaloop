use super::*;
#[cfg(target_arch = "wasm32")]
mod browser;
#[cfg(target_arch = "wasm32")]
pub(super) use browser::{CodecExecutor, CodecJob};
#[cfg(not(target_arch = "wasm32"))]
pub(super) struct CodecJob(std::sync::mpsc::Receiver<Result<Value, String>>);
#[cfg(not(target_arch = "wasm32"))]
pub(super) struct CodecExecutor;
#[cfg(not(target_arch = "wasm32"))]
impl CodecExecutor {
    pub fn new() -> Self {
        Self
    }
    pub fn reset(&mut self) {}
    pub fn start(&mut self, work: Work) -> Result<CodecJob> {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("session-file-codec".into())
            .spawn(move || {
                let _ = tx.send(work.run().map_err(|e| e.to_string()));
            })?;
        Ok(CodecJob(rx))
    }
}
#[cfg(not(target_arch = "wasm32"))]
impl CodecJob {
    pub fn poll(&mut self) -> Option<Result<Value, String>> {
        match self.0.try_recv() {
            Ok(value) => Some(value),
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(_) => Some(Err("file codec worker stopped without a result".into())),
        }
    }
}
