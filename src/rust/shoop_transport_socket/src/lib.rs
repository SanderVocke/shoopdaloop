//! Bounded socket records and authenticated local connection setup.
#![forbid(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]
pub mod setup;
use shoop_transport::{CarrierError, CodecError, Lane, RecordPort, MAX_RECORD_BYTES};
use std::{
    collections::VecDeque,
    io::{self, Read, Write},
    net::{Shutdown, TcpStream},
};

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub reliable: usize,
    pub telemetry: usize,
    pub bulk: usize,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            reliable: 512,
            telemetry: 256,
            bulk: 32,
        }
    }
}
#[derive(Clone)]
struct Record {
    bytes: [u8; MAX_RECORD_BYTES],
    len: usize,
}
pub struct Port {
    stream: TcpStream,
    queues: [VecDeque<Record>; 3],
    capacities: [usize; 3],
    active: Option<(usize, Record, usize)>,
    input: [u8; MAX_RECORD_BYTES],
    used: usize,
    failure: Option<CarrierError>,
}
fn lane_index(lane: Lane) -> usize {
    match lane {
        Lane::Reliable => 0,
        Lane::Telemetry => 1,
        Lane::Bulk => 2,
    }
}
impl Port {
    pub fn from_stream(stream: TcpStream, config: Config) -> io::Result<Self> {
        if config.reliable == 0 || config.bulk == 0 {
            return Err(io::Error::other("zero reliable/bulk capacity"));
        }
        stream.set_nonblocking(true)?;
        let capacities = [config.reliable, config.telemetry, config.bulk];
        Ok(Self {
            stream,
            queues: std::array::from_fn(|i| VecDeque::with_capacity(capacities[i])),
            capacities,
            active: None,
            input: [0; MAX_RECORD_BYTES],
            used: 0,
            failure: None,
        })
    }
    pub fn stream(&self) -> &TcpStream {
        &self.stream
    }
    pub fn outgoing_empty(&self) -> bool {
        self.active.is_none() && self.queues.iter().all(VecDeque::is_empty)
    }
    pub fn close(&mut self) {
        let _ = self.stream.shutdown(Shutdown::Both);
        self.failure = Some(CarrierError::Disconnected);
    }
    pub fn flush(&mut self) -> Result<(), CarrierError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        for _ in 0..32 {
            if self.active.is_none() {
                for lane in [0, 2, 1] {
                    if let Some(record) = self.queues[lane].pop_front() {
                        self.active = Some((lane, record, 0));
                        break;
                    }
                }
            }
            let Some((_, record, offset)) = self.active.as_mut() else {
                return Ok(());
            };
            match self.stream.write(&record.bytes[*offset..record.len]) {
                Ok(0) => {
                    self.failure = Some(CarrierError::Disconnected);
                    return Err(CarrierError::Disconnected);
                }
                Ok(n) => {
                    *offset += n;
                    if *offset == record.len {
                        self.active = None;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    self.failure = Some(CarrierError::Disconnected);
                    return Err(CarrierError::Disconnected);
                }
            }
        }
        Ok(())
    }
    pub fn pending(&self) -> usize {
        self.queues.iter().map(VecDeque::len).sum::<usize>() + usize::from(self.active.is_some())
    }
}
impl RecordPort for Port {
    fn post(&mut self, record: &[u8]) -> Result<(), CarrierError> {
        self.post_on_lane(record, Lane::Reliable)
    }
    fn post_on_lane(&mut self, record: &[u8], lane: Lane) -> Result<(), CarrierError> {
        if record.len() < 2
            || record.len() > MAX_RECORD_BYTES
            || usize::from(u16::from_le_bytes([record[0], record[1]])) != record.len()
        {
            return Err(CarrierError::Codec(CodecError::Invalid));
        }
        self.flush()?;
        let index = lane_index(lane);
        let active = usize::from(self.active.as_ref().is_some_and(|(i, _, _)| *i == index));
        if self.queues[index].len() + active >= self.capacities[index] {
            if lane == Lane::Telemetry && !self.queues[index].is_empty() {
                self.queues[index].pop_front();
            } else {
                return Err(CarrierError::Full);
            }
        }
        let mut frame = Record {
            bytes: [0; MAX_RECORD_BYTES],
            len: record.len(),
        };
        frame.bytes[..record.len()].copy_from_slice(record);
        self.queues[index].push_back(frame);
        self.flush()
    }
    fn take(&mut self, buf: &mut [u8]) -> Option<usize> {
        let _ = self.flush();
        if self.failure.is_some() {
            return None;
        }
        for _ in 0..4 {
            let target = if self.used < 2 {
                2
            } else {
                let n = usize::from(u16::from_le_bytes([self.input[0], self.input[1]]));
                if !(2..=MAX_RECORD_BYTES).contains(&n) {
                    self.failure = Some(CarrierError::Codec(CodecError::Invalid));
                    return None;
                }
                n
            };
            if self.used >= 2 && self.used == target {
                if target > buf.len() {
                    self.failure = Some(CarrierError::Codec(CodecError::Overflow));
                    return None;
                }
                buf[..target].copy_from_slice(&self.input[..target]);
                self.used = 0;
                return Some(target);
            }
            match self.stream.read(&mut self.input[self.used..target]) {
                Ok(0) => {
                    self.failure = Some(if self.used == 0 {
                        CarrierError::Disconnected
                    } else {
                        CarrierError::Codec(CodecError::Truncated)
                    });
                    return None;
                }
                Ok(n) => self.used += n,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return None,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    self.failure = Some(CarrierError::Disconnected);
                    return None;
                }
            }
        }
        None
    }
    fn failure(&mut self) -> Option<CarrierError> {
        self.failure
    }
}
