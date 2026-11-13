//! Bounded register capture and exact, ordered replay over any bridge bus.
use super::{Bus, map};
use alloc::{format, string::String, vec::Vec};
use core::fmt::{self, Write};

pub const FORMAT_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C, u32)]
pub enum Access {
    Read { offset: u32, value: u32 },
    Write { offset: u32, value: u32 },
}

/// Allocates once at construction. Overflow preserves the prefix and counts every lost access.
pub struct RecordingBus<B> {
    inner: B,
    capture: Capture,
    limit: usize,
}

#[derive(Default)]
pub struct Capture {
    records: Vec<Access>,
    dropped: u64,
}

impl<B> RecordingBus<B> {
    pub fn new(inner: B, limit: usize) -> Self {
        Self {
            inner,
            capture: Capture {
                records: Vec::with_capacity(limit),
                dropped: 0,
            },
            limit,
        }
    }
    pub fn inner(&self) -> &B {
        &self.inner
    }
    pub fn records(&self) -> &[Access] {
        self.capture.records()
    }
    pub fn dropped(&self) -> u64 {
        self.capture.dropped()
    }
    /// Stops recording and transfers the buffer without allocating or retaining the device.
    pub fn take_capture(&mut self) -> Capture {
        self.limit = 0;
        core::mem::take(&mut self.capture)
    }
    fn record(&mut self, access: Access) {
        if self.capture.records.len() < self.limit {
            self.capture.records.push(access);
        } else {
            self.capture.dropped = self.capture.dropped.saturating_add(1);
        }
    }
    /// Dump only after execution has stopped, so UART output cannot interleave with capture.
    pub fn dump(&self, output: &mut impl Write) -> fmt::Result {
        self.capture.dump(output)
    }
}

impl Capture {
    pub fn records(&self) -> &[Access] {
        &self.records
    }
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
    pub fn dump(&self, output: &mut impl Write) -> fmt::Result {
        writeln!(
            output,
            "SHOOP_FPGA_TRACE_BEGIN {FORMAT_VERSION} {} {} {}",
            map::VERSION_NUMBER,
            self.records.len(),
            self.dropped
        )?;
        for (index, access) in self.records.iter().enumerate() {
            let (kind, offset, value) = match *access {
                Access::Read { offset, value } => ('R', offset, value),
                Access::Write { offset, value } => ('W', offset, value),
            };
            writeln!(
                output,
                "SHOOP_FPGA_BUS {index} {kind} {offset:08x} {value:08x}"
            )?;
        }
        writeln!(output, "SHOOP_FPGA_TRACE_END {}", self.records.len())
    }
}

impl<B: Bus> Bus for RecordingBus<B> {
    fn read(&mut self, offset: u32) -> u32 {
        let value = self.inner.read(offset);
        self.record(Access::Read { offset, value });
        value
    }
    fn write(&mut self, offset: u32, value: u32) {
        self.inner.write(offset, value);
        self.record(Access::Write { offset, value });
    }
}

/// Extracts one complete capture from a UART log. Truncated or overflowed captures are errors.
pub fn parse(log: &str) -> Result<Vec<Access>, String> {
    let mut expected = None;
    let mut ended = false;
    let mut records = Vec::new();
    for (line_number, line) in log.lines().enumerate() {
        let words: Vec<_> = line.split_whitespace().collect();
        let invalid = || format!("invalid FPGA trace at line {}: {line}", line_number + 1);
        match words.first().copied() {
            Some("SHOOP_FPGA_TRACE_BEGIN") => {
                if expected.is_some() || words.len() != 5 {
                    return Err(invalid());
                }
                if words[1].parse::<u32>() != Ok(FORMAT_VERSION)
                    || words[2].parse::<u32>() != Ok(map::VERSION_NUMBER)
                {
                    return Err("unsupported FPGA trace or register map version".into());
                }
                if words[4].parse::<u64>() != Ok(0) {
                    return Err("FPGA trace overflowed or has an invalid dropped count".into());
                }
                expected = Some(words[3].parse::<usize>().map_err(|_| invalid())?);
            }
            Some("SHOOP_FPGA_BUS") => {
                if expected.is_none() || ended || words.len() != 5 {
                    return Err(invalid());
                }
                if words[1].parse::<usize>() != Ok(records.len())
                    || records.len() >= expected.unwrap()
                {
                    return Err(invalid());
                }
                let offset = u32::from_str_radix(words[3], 16).map_err(|_| invalid())?;
                let value = u32::from_str_radix(words[4], 16).map_err(|_| invalid())?;
                if offset >= 4096 || !offset.is_multiple_of(4) {
                    return Err(invalid());
                }
                records.push(match words[2] {
                    "R" => Access::Read { offset, value },
                    "W" => Access::Write { offset, value },
                    _ => return Err(invalid()),
                });
            }
            Some("SHOOP_FPGA_TRACE_END") => {
                if expected != Some(records.len())
                    || ended
                    || words.len() != 2
                    || words[1].parse::<usize>() != Ok(records.len())
                {
                    return Err(invalid());
                }
                ended = true;
            }
            Some(word) if word.starts_with("SHOOP_FPGA_TRACE") || word == "SHOOP_FPGA_BUS" => {
                return Err(invalid());
            }
            _ => {}
        }
    }
    if !ended || records.is_empty() {
        return Err("missing, empty or incomplete FPGA trace".into());
    }
    Ok(records)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mismatch {
    pub index: usize,
    pub offset: u32,
    pub expected: u32,
    pub actual: u32,
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "FPGA read mismatch at access {} offset {:#05x}: expected {:#010x}, got {:#010x}",
            self.index, self.offset, self.expected, self.actual
        )
    }
}

/// Checks every read, including busy polls and timing counters, without masking or retrying.
pub fn replay(bus: &mut impl Bus, records: &[Access]) -> Result<(), Mismatch> {
    for (index, &access) in records.iter().enumerate() {
        match access {
            Access::Write { offset, value } => bus.write(offset, value),
            Access::Read { offset, value } => {
                let actual = bus.read(offset);
                if actual != value {
                    return Err(Mismatch {
                        index,
                        offset,
                        expected: value,
                        actual,
                    });
                }
            }
        }
    }
    Ok(())
}
