//! Host health, independent of the audio renderer and profiling instrumentation.
use crate::{Descriptor, Name, StreamId};
use serde::{Deserialize, Serialize};

pub const SOURCE: u32 = 3;
pub const REPORT_TYPE: u32 = 4;
pub const SCHEMA: u16 = 1;
pub const MAX_TEMPERATURES: usize = 8;
pub const MAX_REPORT_BYTES: usize = 2048;
pub const INTERVAL_MS: u32 = 1000;

pub fn stream(epoch: u64) -> StreamId {
    StreamId {
        source: SOURCE,
        epoch,
        report_type: REPORT_TYPE,
        schema: SCHEMA,
    }
}

pub fn descriptor(epoch: u64) -> Descriptor {
    Descriptor {
        stream: stream(epoch),
        name: Name::new("host.health").expect("static name"),
        available: true,
        min_interval_ms: INTERVAL_MS,
        max_interval_ms: 60000,
        snapshot: true,
        inspect: false,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Temperature {
    pub name: Name,
    /// None means the sensor could not be read, not zero degrees.
    pub current_millicelsius: Option<i32>,
    /// Sensor-reported maximum; its reset boundary is platform-specific.
    pub peak_millicelsius: Option<i32>,
}

impl Temperature {
    pub fn valid(&self) -> bool {
        self.name.valid()
            && !self.name.as_str().is_empty()
            && [self.current_millicelsius, self.peak_millicelsius]
                .into_iter()
                .flatten()
                .all(|value| (-273150..=250000).contains(&value))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// No populated slots explicitly means this host supplies no temperature sensors.
    pub temperatures: [Option<Temperature>; MAX_TEMPERATURES],
    pub overtemperature: Option<bool>,
}

impl Snapshot {
    pub fn valid(&self) -> bool {
        self.temperatures.iter().enumerate().all(|(index, sensor)| {
            sensor.as_ref().is_none_or(|sensor| {
                sensor.valid()
                    && !self.temperatures[..index]
                        .iter()
                        .flatten()
                        .any(|earlier| earlier.name == sensor.name)
            })
        })
    }
}
