//! Protocol-independent bounded byte delivery. No application or engine semantics.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(test)]
extern crate std;

#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

pub mod buffer;
pub mod framing;

/// Largest record accepted by the shared carriers, including its length prefix.
pub const MAX_RECORD_BYTES: usize = 1280;

/// Capacity class of a message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Lane {
    /// Requests, results, lifecycle and state: never dropped.
    Reliable,
    /// Latest values: a newer record supersedes an undelivered older one.
    Telemetry,
    /// Chunked reads, limited by bulk credits.
    Bulk,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecError {
    /// The output buffer is too small.
    Overflow,
    /// The record ended early.
    Truncated,
    /// A tag, length or value is invalid.
    Invalid,
    /// Bytes remain after the message.
    Trailing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CarrierError {
    /// The carrier cannot take more now; retry later.
    Full,
    Disconnected,
    Codec(CodecError),
}

/// A bidirectional transport of encoded records.
pub trait RecordPort {
    fn post(&mut self, record: &[u8]) -> Result<(), CarrierError>;
    fn post_on_lane(&mut self, record: &[u8], _lane: Lane) -> Result<(), CarrierError> {
        self.post(record)
    }
    /// Terminal carrier failure, distinct from an idle receive.
    fn failure(&mut self) -> Option<CarrierError> {
        None
    }

    /// Copy the next received record into `buf` and return its length.
    fn take(&mut self, buf: &mut [u8]) -> Option<usize>;
}
