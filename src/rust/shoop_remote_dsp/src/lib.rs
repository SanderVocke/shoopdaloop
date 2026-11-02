#![no_std]
extern crate alloc;
pub mod returns;
pub mod stream;
pub mod wire;

use shoop_dsp::Config as DspConfig;

fn slot_index(sequence: u64, capacity: usize) -> usize {
    (sequence % capacity as u64) as usize
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Error {
    Config,
    Mtu,
    Deadline,
    Packet,
    Capacity,
    Stale,
    Duplicate,
    Future,
    Dsp,
    Retired,
    Io,
}
impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "remote DSP {self:?}")
    }
}
impl core::error::Error for Error {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct Config {
    pub start_frame: u64,
    pub sample_rate: u32,
    pub packet_frames: u16,
    pub multiplier: u16,
    pub allowance: u16,
    pub channels: u16,
    pub mtu: u16,
    pub midi_per_packet: u16,
    pub reorder_packets: u16,
    pub gap_timeout_us: u32,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            start_frame: 0,
            sample_rate: 48_000,
            packet_frames: 16,
            multiplier: 4,
            allowance: 8,
            channels: 2,
            mtu: 1500,
            midi_per_packet: 4,
            reorder_packets: 8,
            gap_timeout_us: 200,
        }
    }
}
impl Config {
    pub fn dsp(self) -> DspConfig {
        DspConfig {
            sample_rate: self.sample_rate,
            frames: self.packet_frames as usize * self.multiplier as usize,
            channels: self.channels as usize,
        }
    }
    pub fn validate(self) -> Result<Self, Error> {
        self.dsp().validate().map_err(|_| Error::Config)?;
        if self
            .start_frame
            .checked_add(self.dsp().frames as u64)
            .is_none()
        {
            return Err(Error::Config);
        }
        if self.multiplier == 0
            || self.multiplier > 64
            || self.allowance > 4096
            || self.reorder_packets > 128
            || self.midi_per_packet > 16
            || self.gap_timeout_us == 0
            || self.gap_timeout_us > 1_000_000
        {
            return Err(Error::Config);
        }
        if self.allowance < self.multiplier {
            return Err(Error::Deadline);
        }
        if self.mtu < 1280 || self.max_packet_bytes() > self.mtu as usize - 48 {
            return Err(Error::Mtu);
        }
        Ok(self)
    }
    pub fn max_packet_bytes(self) -> usize {
        wire::HEADER_BYTES
            + self.packet_frames as usize * self.channels as usize * 4
            + self.midi_per_packet as usize * wire::MIDI_BYTES
    }
    pub fn packet_ns(self) -> u64 {
        self.packet_frames as u64 * 1_000_000_000 / self.sample_rate as u64
    }
    pub fn ready_ns(self, sequence: u64) -> Result<u64, Error> {
        if self.sample_rate == 0 {
            return Err(Error::Config);
        }
        let frame = sequence
            .checked_add(1)
            .and_then(|n| n.checked_mul(self.packet_frames as u64))
            .ok_or(Error::Capacity)?;
        u64::try_from(frame as u128 * 1_000_000_000 / self.sample_rate as u128)
            .map_err(|_| Error::Capacity)
    }
    pub fn deadline_ns(self, sequence: u64) -> Result<u64, Error> {
        self.ready_ns(sequence)?
            .checked_add(self.allowance as u64 * self.packet_ns())
            .ok_or(Error::Capacity)
    }
}
