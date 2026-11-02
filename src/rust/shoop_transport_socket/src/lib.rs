//! Native TCP adapter for bounded portable record progression.
#![forbid(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]
pub mod setup;
pub use shoop_transport::stream::Config;
use shoop_transport::{
    stream::{ByteIo, IoError, Stream},
    CarrierError, Lane, RecordPort,
};
use std::{
    io::{self, Read, Write},
    net::{Shutdown, TcpStream},
};

pub struct NativeStream(pub TcpStream);
fn convert(error: io::Error) -> IoError {
    match error.kind() {
        io::ErrorKind::WouldBlock => IoError::WouldBlock,
        io::ErrorKind::Interrupted => IoError::Interrupted,
        _ => IoError::Disconnected,
    }
}
impl ByteIo for NativeStream {
    fn try_read(&mut self, output: &mut [u8]) -> Result<usize, IoError> {
        self.0.read(output).map_err(convert)
    }
    fn try_write(&mut self, input: &[u8]) -> Result<usize, IoError> {
        self.0.write(input).map_err(convert)
    }
}
pub struct Port {
    core: Stream<NativeStream>,
}
impl Port {
    pub fn from_stream(stream: TcpStream, config: Config) -> io::Result<Self> {
        stream.set_nodelay(true)?;
        stream.set_nonblocking(true)?;
        Ok(Self {
            core: Stream::new(NativeStream(stream), config)
                .map_err(|_| io::Error::other("invalid stream capacities"))?,
        })
    }
    pub fn stream(&self) -> &TcpStream {
        &self.core.io().0
    }
    pub fn outgoing_empty(&self) -> bool {
        self.core.outgoing_empty()
    }
    pub fn close(&mut self) {
        let _ = self.stream().shutdown(Shutdown::Both);
        self.core.close();
    }
    pub fn flush(&mut self) -> Result<(), CarrierError> {
        self.core.flush()
    }
    pub fn pending(&self) -> usize {
        self.core.pending()
    }
}
impl RecordPort for Port {
    fn flush(&mut self) -> Result<(), CarrierError> {
        Self::flush(self)
    }
    fn pending_records(&self) -> usize {
        self.pending()
    }
    fn post(&mut self, record: &[u8]) -> Result<(), CarrierError> {
        self.core.post(record)
    }
    fn post_on_lane(&mut self, record: &[u8], lane: Lane) -> Result<(), CarrierError> {
        self.core.post_on_lane(record, lane)
    }
    fn take(&mut self, output: &mut [u8]) -> Option<usize> {
        self.core.take(output)
    }
    fn failure(&mut self) -> Option<CarrierError> {
        self.core.failure()
    }
}
