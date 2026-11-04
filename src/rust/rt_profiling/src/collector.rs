use alloc::{collections::BTreeMap, format, string::String, vec::Vec};
use serde::{Deserialize, Serialize};

use crate::{
    Packet, Phase, Sample, Status, CALLBACK, CAPTURE_END, FAULT, MAX_SAMPLES, PHASE, TRUNCATED,
};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Statistics {
    pub count: u64,
    pub mean: f64,
    pub m2: f64,
    pub worst: u64,
}
impl Statistics {
    pub fn add(&mut self, value: u64) {
        self.count += 1;
        let delta = value as f64 - self.mean;
        self.mean += delta / self.count as f64;
        self.m2 += delta * (value as f64 - self.mean);
        self.worst = self.worst.max(value);
    }
    pub fn stddev(&self) -> f64 {
        if self.count == 0 {
            0.
        } else {
            libm::sqrt((self.m2 / self.count as f64).max(0.))
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub key: u64,
    pub name: String,
    pub samples: u64,
    pub avg_ns: f64,
    pub worst_ns: u64,
    pub stddev_ns: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Interval {
    pub start_ns: u64,
    pub end_ns: u64,
    pub first_frame: u64,
    pub end_frame: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub generation: u64,
    pub capture: u64,
    pub trigger: u32,
    pub interval: Interval,
    pub rows: Vec<Row>,
    pub incomplete: bool,
    pub complete: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub generation: u64,
    pub requested: bool,
    pub active: bool,
    pub available: bool,
    pub resolution_ns: u64,
    pub dropped: u64,
    pub skipped_captures: u64,
    pub high_water: u64,
    pub storage_bytes: u64,
    pub live: Report,
    pub capture: Option<Report>,
}

#[derive(Default)]
struct Measured {
    interval: Option<Interval>,
    order: Vec<u64>,
    values: BTreeMap<u64, Statistics>,
    incomplete: bool,
}
impl Measured {
    fn accept(&mut self, packet: Packet, samples: &[Sample]) -> bool {
        let mut previous = packet.start_ns;
        for sample in samples {
            if sample.at < previous
                || sample.at > packet.end_ns
                || (sample.key & PHASE == 0 && (sample.key & !crate::LAYOUT) >= MAX_SAMPLES as u64)
            {
                self.incomplete = true;
                return false;
            }
            previous = sample.at;
        }
        let interval = self.interval.get_or_insert(Interval {
            start_ns: packet.start_ns,
            end_ns: packet.end_ns,
            first_frame: packet.frame,
            end_frame: packet.frame.saturating_add(u64::from(packet.frames)),
        });
        interval.start_ns = interval.start_ns.min(packet.start_ns);
        interval.end_ns = interval.end_ns.max(packet.end_ns);
        interval.first_frame = interval.first_frame.min(packet.frame);
        interval.end_frame = interval
            .end_frame
            .max(packet.frame.saturating_add(u64::from(packet.frames)));
        self.incomplete |= packet.flags & (FAULT | TRUNCATED) != 0;
        if packet.flags & CALLBACK != 0 {
            return true;
        }
        let mut previous = None;
        for sample in samples {
            if sample.key == PHASE | Phase::GraphStart as u64 {
                previous = Some(sample.at);
            }
            if sample.key & crate::LAYOUT != 0 && sample.key & PHASE == 0 {
                let key = sample.key & !crate::LAYOUT;
                if !self.values.contains_key(&key) {
                    self.order.push(key);
                    self.values.insert(key, Statistics::default());
                }
            } else if sample.key & PHASE == 0 {
                let Some(start) = previous else {
                    self.incomplete = true;
                    return false;
                };
                let entry = self.values.entry(sample.key).or_insert_with(|| {
                    self.order.push(sample.key);
                    Statistics::default()
                });
                entry.add(sample.at - start);
                previous = Some(sample.at);
            }
        }
        true
    }
    fn report(
        &self,
        generation: u64,
        capture: u64,
        trigger: u32,
        complete: bool,
        names: &BTreeMap<u64, String>,
    ) -> Report {
        Report {
            generation,
            capture,
            trigger,
            complete,
            interval: self.interval.clone().unwrap_or_default(),
            incomplete: self.incomplete,
            rows: self
                .order
                .iter()
                .map(|key| {
                    let value = &self.values[key];
                    Row {
                        key: *key,
                        name: names
                            .get(key)
                            .cloned()
                            .unwrap_or_else(|| format!("Node {key}")),
                        samples: value.count,
                        avg_ns: value.mean,
                        worst_ns: value.worst,
                        stddev_ns: value.stddev(),
                    }
                })
                .collect(),
        }
    }
}

#[derive(Default)]
struct Assembly {
    header: Option<Packet>,
    samples: Vec<Sample>,
}
impl Assembly {
    fn push(&mut self, packet: Packet) -> Option<(Packet, Vec<Sample>)> {
        if !packet.valid() {
            self.header = None;
            self.samples.clear();
            return None;
        }
        if packet.index == 0 {
            self.header = Some(packet);
            self.samples.clear();
        }
        let header = self.header?;
        if header.generation != packet.generation
            || header.capture != packet.capture
            || header.sequence != packet.sequence
            || header.callback != packet.callback
            || header.callback_frame != packet.callback_frame
            || header.frames != packet.frames
            || (header.flags & !CAPTURE_END) != (packet.flags & !CAPTURE_END)
            || header.trigger != packet.trigger
            || header.epoch != packet.epoch
            || header.frame != packet.frame
            || header.total != packet.total
            || header.start_ns != packet.start_ns
            || header.end_ns != packet.end_ns
            || self.samples.len() != packet.index as usize
        {
            self.header = None;
            self.samples.clear();
            return None;
        }
        self.samples
            .extend_from_slice(&packet.samples[..usize::from(packet.len)]);
        if self.samples.len() == packet.total as usize {
            self.header = None;
            Some((packet, core::mem::take(&mut self.samples)))
        } else {
            None
        }
    }
}

#[derive(Default)]
pub struct Collector {
    status: Status,
    names: BTreeMap<u64, String>,
    live: Measured,
    capture: Measured,
    live_assembly: Assembly,
    capture_assembly: Assembly,
    capture_id: u64,
    trigger: u32,
    capture_complete: bool,
    raw: Vec<Packet>,
    published_at: u64,
    dirty: bool,
    view: View,
}
impl Collector {
    pub fn status(&mut self, status: Status) {
        if status.generation < self.status.generation {
            return;
        }
        if status.generation != self.status.generation {
            *self = Self {
                status,
                dirty: true,
                ..Self::default()
            };
        } else {
            if status.active != self.status.active
                || status.requested != self.status.requested
                || status.available != self.status.available
            {
                self.dirty = true;
            }
            self.status = status;
        }
    }
    pub fn name(&mut self, generation: u64, key: u64, name: &str) {
        if generation == self.status.generation && key < MAX_SAMPLES as u64 {
            self.names.insert(key, name.into());
        }
    }
    pub fn receive(&mut self, packet: Packet) {
        if packet.generation != self.status.generation || !packet.valid() {
            return;
        }
        if packet.capture != 0 {
            if packet.capture < self.capture_id {
                return;
            }
            if packet.capture != self.capture_id {
                self.capture_id = packet.capture;
                self.capture = Measured::default();
                self.capture_assembly = Assembly::default();
                self.raw.clear();
                self.capture_complete = false;
            }
            self.trigger = packet.trigger;
            if self.raw.len() < 16 * 1024 * 1024 / core::mem::size_of::<Packet>() {
                self.raw.push(packet);
            } else {
                self.capture.incomplete = true;
            }
            if let Some((header, samples)) = self.capture_assembly.push(packet) {
                self.capture.accept(header, &samples);
                if header.flags & CAPTURE_END != 0 {
                    self.capture_complete = true;
                    self.dirty = true;
                }
            }
        } else {
            if !self.status.active {
                self.dirty = true;
            }
            if let Some(header) = self.live_assembly.header {
                if header.sequence != packet.sequence {
                    self.live.incomplete = true;
                }
            }
            if let Some((header, samples)) = self.live_assembly.push(packet) {
                self.live.accept(header, &samples);
            }
        }
    }
    pub fn raw_capture(&self) -> &[Packet] {
        &self.raw
    }
    pub fn view(&mut self) -> View {
        let end = self.live.interval.as_ref().map_or(0, |i| i.end_ns);
        if self.dirty || end.saturating_sub(self.published_at) >= 200_000_000 {
            self.published_at = end;
            self.dirty = false;
            self.view = View {
                generation: self.status.generation,
                requested: self.status.requested,
                active: self.status.active,
                available: self.status.available,
                resolution_ns: self.status.resolution_ns,
                dropped: self.status.dropped,
                skipped_captures: self.status.skipped_captures,
                high_water: self.status.high_water,
                storage_bytes: self.status.storage_bytes,
                live: self.live.report(
                    self.status.generation,
                    0,
                    0,
                    !self.status.active,
                    &self.names,
                ),
                capture: (self.capture_id != 0).then(|| {
                    self.capture.report(
                        self.status.generation,
                        self.capture_id,
                        self.trigger,
                        self.capture_complete,
                        &self.names,
                    )
                }),
            };
        }
        self.view.dropped = self.status.dropped;
        self.view.high_water = self.status.high_water;
        self.view.storage_bytes = self.status.storage_bytes;
        self.view.skipped_captures = self.status.skipped_captures;
        self.view.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test]
    fn population_statistics_match_independent_values() {
        let mut s = Statistics::default();
        assert_eq!(s.stddev(), 0.);
        for value in [2, 4, 4, 4, 5, 5, 7, 9] {
            s.add(value);
        }
        assert_eq!(s.count, 8);
        assert_eq!(s.mean, 5.);
        assert_eq!(s.worst, 9);
        assert!((s.stddev() - 2.).abs() < 1e-12);
        let mut large = Statistics::default();
        for value in [1_000_000_000_000, 1_000_000_000_002] {
            large.add(value);
        }
        assert_eq!(large.stddev(), 1.);
    }

    fn packet(generation: u64, sequence: u64, cost: u64) -> Packet {
        let mut p = Packet {
            generation,
            sequence,
            start_ns: 10,
            end_ns: 10 + cost,
            total: 2,
            len: 2,
            ..Packet::default()
        };
        p.samples[0] = Sample {
            key: PHASE | Phase::GraphStart as u64,
            at: 10,
        };
        p.samples[1] = Sample {
            key: 3,
            at: 10 + cost,
        };
        p
    }

    #[shoop_test]
    fn resets_refuse_stale_rows_and_partial_chunks() {
        let mut collector = Collector::default();
        collector.status(Status {
            generation: 1,
            ..Status::default()
        });
        collector.receive(packet(1, 1, 10));
        collector.receive(packet(1, 2, 30));
        let view = collector.view();
        assert_eq!(view.live.rows[0].avg_ns, 20.);
        assert_eq!(view.live.rows[0].stddev_ns, 10.);
        collector.status(Status {
            generation: 2,
            ..Status::default()
        });
        collector.receive(packet(1, 3, 100));
        assert!(collector.view().live.rows.is_empty());
        let mut partial = packet(2, 4, 999);
        partial.len = 1;
        collector.receive(partial);
        let mut tail = partial;
        tail.index = 1;
        tail.sequence = 5;
        collector.receive(tail);
        assert!(collector.view().live.rows.is_empty());
        collector.receive(packet(2, 6, 50));
        collector.status(Status {
            generation: 2,
            available: true,
            ..Status::default()
        });
        assert_eq!(collector.view().live.rows[0].worst_ns, 50);
    }

    #[shoop_test]
    fn malformed_keys_and_inconsistent_chunk_metadata_are_rejected() {
        let mut collector = Collector::default();
        collector.status(Status {
            generation: 1,
            ..Status::default()
        });
        for field in 0..4 {
            let mut first = packet(1, field + 1, 10);
            first.len = 1;
            let mut tail = first;
            tail.index = 1;
            tail.samples[0] = Sample { key: 3, at: 20 };
            match field {
                0 => tail.callback_frame += 1,
                1 => tail.frames += 1,
                2 => tail.flags ^= FAULT,
                _ => tail.trigger ^= 1,
            }
            collector.receive(first);
            collector.receive(tail);
        }
        let mut invalid = packet(1, 6, 10);
        invalid.samples[1].key = (1u64 << 32) | 3;
        collector.receive(invalid);
        assert!(collector.view().live.rows.is_empty());
        collector.receive(packet(1, 7, 10));
        assert_eq!(collector.view().live.rows[0].samples, 1);
    }

    #[shoop_test]
    fn unfinished_nodes_keep_names_without_fabricating_duration_samples() {
        let mut collector = Collector::default();
        collector.status(Status {
            generation: 1,
            ..Status::default()
        });
        collector.name(1, 4, "Capture");
        let mut p = Packet {
            generation: 1,
            sequence: 1,
            start_ns: 10,
            end_ns: 50,
            total: 4,
            len: 4,
            flags: FAULT,
            ..Packet::default()
        };
        p.samples[0] = Sample {
            key: PHASE | Phase::GraphStart as u64,
            at: 10,
        };
        p.samples[1] = Sample {
            key: crate::LAYOUT | 3,
            at: 10,
        };
        p.samples[2] = Sample {
            key: crate::LAYOUT | 4,
            at: 10,
        };
        p.samples[3] = Sample { key: 3, at: 30 };
        collector.receive(p);
        let view = collector.view();
        assert!(view.live.incomplete);
        assert_eq!(view.live.rows.len(), 2);
        assert_eq!(view.live.rows[0].avg_ns, 20.);
        assert_eq!(view.live.rows[1].name, "Capture");
        assert_eq!(view.live.rows[1].samples, 0);
    }
}
