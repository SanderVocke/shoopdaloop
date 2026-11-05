use crate::graph::GraphAssembly;
use crate::{
    Boundary, CapturePage, CaptureRequest, GraphRecord, NodeTiming, Topology, TraceSummary,
    DETAIL_NODES, MAX_CAPTURE_TRACES,
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    format,
    string::String,
    sync::Arc,
    vec::Vec,
};
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
    pub ready_avg_ns: f64,
    pub ready_worst_ns: u64,
    pub ready_stddev_ns: f64,
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
    pub epoch: u64,
    pub capture: u64,
    pub trigger: u32,
    pub interval: Interval,
    pub rows: Vec<Row>,
    pub incomplete: bool,
    pub complete: bool,
    pub wrapping: crate::WrappingStats,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub kind: u8,
    pub slot: u32,
    pub entity: u64,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub connection: u64,
    #[serde(skip)]
    pub lease: u128,
    #[serde(skip)]
    pub attachment: u64,
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
    pub topology: Option<Arc<Topology>>,
    pub groups: Arc<[Group]>,
    pub index: Arc<[TraceSummary]>,
    pub index_incomplete: bool,
    pub graph_limited: bool,
    pub detail: Option<Arc<CapturePage>>,
}

impl View {
    pub fn node_names(&self, report: &Report) -> BTreeMap<u64, String> {
        let groups: BTreeMap<_, _> = self
            .groups
            .iter()
            .filter(|g| !g.name.is_empty())
            .map(|g| ((g.kind, g.slot), &g.name))
            .collect();
        let prefixes: BTreeMap<_, _> = self
            .topology
            .as_ref()
            .filter(|t| t.generation == report.generation && t.epoch == report.epoch)
            .into_iter()
            .flat_map(|t| &t.nodes)
            .filter_map(|n| groups.get(&(n.group, n.slot)).map(|name| (n.key, *name)))
            .collect();
        report
            .rows
            .iter()
            .map(|row| {
                let name = prefixes
                    .get(&row.key)
                    .map_or_else(|| row.name.clone(), |name| format!("{name} · {}", row.name));
                (row.key, name)
            })
            .collect()
    }

    pub fn valid(&self) -> bool {
        let report = |r: &Report| {
            r.wrapping.valid()
                && r.rows.len() <= MAX_SAMPLES
                && r.rows.iter().all(|n| {
                    n.key < MAX_SAMPLES as u64
                        && n.name.len() <= 128
                        && n.avg_ns.is_finite()
                        && n.stddev_ns.is_finite()
                        && n.ready_avg_ns.is_finite()
                        && n.ready_stddev_ns.is_finite()
                })
        };
        self.groups.len() <= crate::MAX_GRAPH_NODES
            && self
                .groups
                .iter()
                .all(|g| (1..=3).contains(&g.kind) && g.name.len() <= 2048)
            && report(&self.live)
            && self.capture.as_ref().is_none_or(report)
            && self
                .topology
                .as_ref()
                .is_none_or(|t| t.generation == self.generation && t.valid())
            && self.index.len() <= MAX_CAPTURE_TRACES
            && self.index.iter().all(|t| t.start_ns <= t.end_ns)
            && self.detail.as_ref().is_none_or(|d| {
                d.request.connection == self.connection
                    && d.request.generation == self.generation
                    && self
                        .capture
                        .as_ref()
                        .is_some_and(|r| r.capture == d.request.capture)
                    && d.nodes.len() <= DETAIL_NODES
                    && d.phases.len() <= 64
                    && d.total as usize <= MAX_SAMPLES
                    && d.nodes
                        .iter()
                        .all(|n| n.key < MAX_SAMPLES as u64 && n.start_ns <= n.end_ns)
            })
    }
}

#[derive(Default)]
struct Measured {
    interval: Option<Interval>,
    order: Vec<u64>,
    values: BTreeMap<u64, Statistics>,
    readiness: BTreeMap<u64, Statistics>,
    wrapping: crate::wrapping::Wrapping,
    epoch: u64,
    incomplete: bool,
}
impl Measured {
    fn accept(&mut self, packet: Packet, samples: &[Sample]) -> bool {
        if !valid_samples(packet, samples) {
            self.incomplete = true;
            return false;
        }
        if !self.values.is_empty() && self.epoch != packet.epoch {
            self.incomplete = true;
            return false;
        }
        self.wrapping
            .accept(TraceSummary::from_packet(packet, samples));
        self.epoch = packet.epoch;
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
        let mut graph_start = None;
        for sample in samples {
            if sample.key == PHASE | Phase::GraphStart as u64 {
                previous = Some(sample.at);
                graph_start = previous;
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
                self.readiness
                    .entry(sample.key)
                    .or_default()
                    .add(sample.at - graph_start.unwrap());
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
            wrapping: self.wrapping.stats.clone(),
            epoch: self.epoch,
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
                    let ready = self.readiness.get(key).cloned().unwrap_or_default();
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
                        ready_avg_ns: ready.mean,
                        ready_worst_ns: ready.worst,
                        ready_stddev_ns: ready.stddev(),
                    }
                })
                .collect(),
        }
    }
}

fn valid_samples(packet: Packet, samples: &[Sample]) -> bool {
    let mut previous = packet.start_ns;
    let mut graph = false;
    let mut keys = BTreeSet::new();
    for sample in samples {
        if sample.at < previous || sample.at > packet.end_ns {
            return false;
        }
        previous = sample.at;
        if sample.key == PHASE | Phase::GraphStart as u64 {
            if graph {
                return false;
            }
            graph = true;
        }
        if sample.key & PHASE == 0 {
            if sample.key & !crate::LAYOUT >= MAX_SAMPLES as u64 {
                return false;
            }
            if sample.key & crate::LAYOUT == 0 && (!graph || !keys.insert(sample.key)) {
                return false;
            }
        }
    }
    true
}

#[derive(Default)]
struct Assembly {
    header: Option<Packet>,
    samples: Vec<Sample>,
    lost: bool,
}
impl Assembly {
    fn push(&mut self, packet: Packet) -> Option<(Packet, Vec<Sample>)> {
        if !packet.valid() {
            self.header = None;
            self.samples.clear();
            self.lost = true;
            return None;
        }
        if packet.index == 0 {
            self.lost |= self.header.is_some();
            self.header = Some(packet);
            self.samples.clear();
        }
        let Some(header) = self.header else {
            self.lost = true;
            return None;
        };
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
            || header.graphs != packet.graphs
            || header.budget_ns != packet.budget_ns
            || self.samples.len() != packet.index as usize
        {
            self.lost = true;
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
    connection: u64,
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
    index: Vec<TraceSummary>,
    index_incomplete: bool,
    graph_assembly: GraphAssembly,
    topology: Option<Arc<Topology>>,
    graph_limited: bool,
    published_at: u64,
    dirty: bool,
    view: View,
}
impl Collector {
    pub fn set_connection(&mut self, connection: u64) {
        *self = Self {
            connection,
            dirty: true,
            ..Self::default()
        };
    }
    pub fn status(&mut self, status: Status) {
        if status.generation < self.status.generation {
            return;
        }
        if status.generation != self.status.generation {
            *self = Self {
                connection: self.connection,
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
            if self.names.get(&key).is_none_or(|old| old != name) {
                self.names.insert(key, name.into());
                self.dirty = true;
            }
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
                self.index.clear();
                self.index_incomplete = false;
                self.view.detail = None;
                self.view.index = Arc::from([]);
                self.capture_complete = false;
            }
            self.trigger = packet.trigger;
            if self.raw.len() < 16 * 1024 * 1024 / core::mem::size_of::<Packet>() {
                self.raw.push(packet);
            } else {
                self.capture.incomplete = true;
            }
            let assembled = self.capture_assembly.push(packet);
            self.capture.incomplete |= core::mem::take(&mut self.capture_assembly.lost);
            if let Some((header, samples)) = assembled {
                if self.capture.accept(header, &samples) {
                    if self.index.len() < MAX_CAPTURE_TRACES {
                        self.index.push(TraceSummary::from_packet(header, &samples));
                    } else {
                        self.index_incomplete = true;
                    }
                }
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
            let assembled = self.live_assembly.push(packet);
            self.live.incomplete |= core::mem::take(&mut self.live_assembly.lost);
            if let Some((header, samples)) = assembled {
                self.live.accept(header, &samples);
                if self
                    .topology
                    .as_ref()
                    .is_some_and(|t| t.epoch != header.epoch)
                {
                    self.topology = None;
                    self.dirty = true;
                }
            }
        }
    }
    pub fn graph(&mut self, generation: u64, epoch: u64, record: GraphRecord) {
        if generation != self.status.generation {
            return;
        }
        if record == GraphRecord::Unavailable {
            self.graph_limited = true;
            self.topology = None;
            if self.live.values.values().all(|v| v.count == 0) {
                self.live = Measured::default();
            }
            self.dirty = true;
            return;
        }
        if let Some(topology) = self.graph_assembly.accept(generation, epoch, record) {
            let unmeasured = self.live.values.values().all(|v| v.count == 0);
            if unmeasured || topology.epoch == self.live.epoch {
                if unmeasured {
                    self.live = Measured::default();
                    self.live.epoch = topology.epoch;
                    for node in &topology.nodes {
                        self.live.order.push(node.key);
                        self.live.values.insert(node.key, Statistics::default());
                    }
                }
                self.topology = Some(Arc::new(topology));
                self.graph_limited = false;
                self.dirty = true;
            }
        }
    }
    pub fn select(&mut self, request: CaptureRequest) -> bool {
        if request.connection != self.connection
            || request.generation != self.status.generation
            || request.capture != self.capture_id
            || request.capture == 0
            || request.offset as usize > MAX_SAMPLES
        {
            return false;
        }
        if self
            .view
            .detail
            .as_ref()
            .is_some_and(|p| p.request == request)
        {
            return true;
        }
        let Some(callback) = self
            .index
            .iter()
            .find(|t| t.sequence == request.callback && t.flags & CALLBACK != 0)
            .copied()
        else {
            return false;
        };
        let tick = if request.tick == 0 {
            None
        } else {
            let Some(tick) = self
                .index
                .iter()
                .find(|t| {
                    t.sequence == request.tick && t.flags & CALLBACK == 0 && callback.contains(t)
                })
                .copied()
            else {
                return false;
            };
            Some(tick)
        };
        let mut page = CapturePage {
            request,
            wrapping: crate::Envelope::from_traces(callback, self.index.iter()),
            callback,
            tick,
            incomplete: self.capture.incomplete || self.index_incomplete,
            ..CapturePage::default()
        };
        let mut assembly = Assembly::default();
        for packet in &self.raw {
            if packet.sequence != request.callback && packet.sequence != request.tick {
                continue;
            }
            if let Some((header, samples)) = assembly.push(*packet) {
                if !valid_samples(header, &samples) {
                    page.incomplete = true;
                    continue;
                }
                let mut graph_start = None;
                let mut previous = None;
                for sample in samples {
                    if sample.key & PHASE != 0 {
                        if page.phases.len() < 64 {
                            page.phases.push(Boundary {
                                phase: sample.key & !PHASE,
                                at: sample.at,
                            });
                        } else {
                            page.incomplete = true;
                        }
                        if sample.key == PHASE | Phase::GraphStart as u64 {
                            graph_start = Some(sample.at);
                            previous = graph_start;
                        }
                    } else if sample.key & crate::LAYOUT == 0 && header.sequence == request.tick {
                        if let (Some(start), Some(graph)) = (previous, graph_start) {
                            if page.total >= request.offset && page.nodes.len() < DETAIL_NODES {
                                page.nodes.push(NodeTiming {
                                    key: sample.key,
                                    start_ns: start,
                                    end_ns: sample.at,
                                    ready_ns: sample.at - graph,
                                });
                            }
                            page.total += 1;
                            previous = Some(sample.at);
                        }
                    }
                }
            }
        }
        page.phases.sort_by_key(|p| p.at);
        self.view.detail = Some(Arc::new(page));
        self.dirty = true;
        true
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
                connection: self.connection,
                lease: 0,
                attachment: 0,
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
                topology: self.topology.clone(),
                groups: Arc::from([]),
                graph_limited: self.graph_limited,
                index: if self.view.index.len() == self.index.len() {
                    self.view.index.clone()
                } else {
                    Arc::from(self.index.clone())
                },
                index_incomplete: self.index_incomplete,
                detail: self.view.detail.clone(),
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

    fn prescribed(sequence: u64, a: u64, b: u64) -> Packet {
        let mut p = Packet {
            generation: 1,
            sequence,
            epoch: 4,
            start_ns: 1_000_000_000,
            end_ns: 1_000_110_000,
            total: 4,
            len: 4,
            frames: 32,
            ..Packet::default()
        };
        p.samples[0] = Sample {
            key: PHASE | Phase::GraphStart as u64,
            at: p.start_ns,
        };
        p.samples[1] = Sample {
            key: 1,
            at: p.start_ns + a,
        };
        p.samples[2] = Sample {
            key: 2,
            at: p.start_ns + a + b,
        };
        p.samples[3] = Sample {
            key: 3,
            at: p.end_ns,
        };
        p
    }

    #[shoop_test]
    fn readiness_includes_independent_work_and_aggregates_correlated_ticks() {
        let mut c = Collector::default();
        c.status(Status {
            generation: 1,
            available: true,
            ..Status::default()
        });
        c.receive(prescribed(1, 20_000, 80_000));
        c.receive(prescribed(2, 100_000, 0));
        let report = c.view().live;
        assert_eq!(report.rows[0].stddev_ns, 40_000.);
        assert_eq!(report.rows[1].stddev_ns, 40_000.);
        assert_eq!(report.rows[2].avg_ns, 10_000.);
        assert_eq!(report.rows[2].ready_avg_ns, 110_000.);
        assert_eq!(report.rows[2].ready_worst_ns, 110_000);
        assert_eq!(report.rows[2].ready_stddev_ns, 0.);
        assert_eq!(report.rows[1].ready_worst_ns, 100_000);
        assert_eq!(report.rows[0].worst_ns + report.rows[1].worst_ns, 180_000);
    }

    #[shoop_test]
    fn graph_publication_is_atomic_and_rejects_stale_or_incomplete_metadata() {
        use crate::{GraphEdge, GraphNode, Port};
        let mut c = Collector::default();
        c.status(Status {
            generation: 1,
            available: true,
            ..Status::default()
        });
        c.receive(prescribed(1, 20_000, 80_000));
        let begin = GraphRecord::Begin {
            nodes: 3,
            edges: 1,
            sample_rate: 48000,
            quantum: 32,
        };
        c.graph(1, 4, begin);
        c.graph(
            1,
            4,
            GraphRecord::Node {
                index: 1,
                node: GraphNode {
                    key: 2,
                    ..GraphNode::default()
                },
            },
        );
        c.graph(1, 4, GraphRecord::End);
        assert!(c.view().topology.is_none());
        c.graph(1, 4, begin);
        for i in 0..3 {
            c.graph(
                1,
                4,
                GraphRecord::Node {
                    index: i,
                    node: GraphNode {
                        key: u64::from(i) + 1,
                        group: 1,
                        slot: 0,
                        owner_generation: 1,
                    },
                },
            );
        }
        let edge = GraphEdge {
            source: Port {
                node: Some(1),
                ..Port::default()
            },
            destination: Port {
                node: Some(3),
                ..Port::default()
            },
        };
        c.graph(1, 4, GraphRecord::Edge { index: 0, edge });
        assert!(c.view().topology.is_none());
        c.graph(1, 4, GraphRecord::End);
        let t = c.view().topology.unwrap();
        assert_eq!(t.edges, [edge]);
        assert!((t.budget_ns().unwrap() - 666_666.6666666666).abs() < 1e-6);
        c.status(Status {
            generation: 2,
            ..Status::default()
        });
        c.graph(1, 4, begin);
        c.graph(1, 4, GraphRecord::End);
        assert!(c.view().topology.is_none());
    }

    #[shoop_test]
    fn topology_updates_without_an_elapsed_clock_and_capacity_is_explicit() {
        let mut c = Collector::default();
        c.status(Status {
            generation: 1,
            requested: true,
            ..Status::default()
        });
        for epoch in [1, 2] {
            c.graph(
                1,
                epoch,
                GraphRecord::Begin {
                    nodes: 1,
                    edges: 0,
                    sample_rate: 48000,
                    quantum: 32,
                },
            );
            c.name(1, epoch, "Prepared processor");
            c.graph(
                1,
                epoch,
                GraphRecord::Node {
                    index: 0,
                    node: crate::GraphNode {
                        key: epoch,
                        ..crate::GraphNode::default()
                    },
                },
            );
            c.graph(1, epoch, GraphRecord::End);
            let view = c.view();
            assert_eq!(view.topology.unwrap().epoch, epoch);
            assert_eq!(view.live.rows.len(), 1);
            assert_eq!(view.live.rows[0].key, epoch);
            assert_eq!(view.live.rows[0].name, "Prepared processor");
            assert_eq!(view.live.rows[0].samples, 0);
        }
        c.graph(1, 2, GraphRecord::Unavailable);
        let view = c.view();
        assert!(view.graph_limited && view.topology.is_none() && view.live.rows.is_empty());
    }

    #[shoop_test]
    fn selected_capture_uses_actual_tick_and_original_callback_budget() {
        let mut c = Collector::default();
        c.set_connection(40);
        c.status(Status {
            generation: 1,
            available: true,
            ..Status::default()
        });
        let mut tick = prescribed(7, 20_000, 80_000);
        tick.capture = 9;
        c.receive(tick);
        let mut callback = Packet {
            generation: 1,
            capture: 9,
            sequence: 8,
            epoch: 4,
            start_ns: tick.start_ns - 100,
            end_ns: tick.end_ns + 100,
            budget_ns: 100_000,
            flags: CALLBACK | crate::HOST | CAPTURE_END,
            ..Packet::default()
        };
        c.receive(callback);
        let request = CaptureRequest {
            connection: 40,
            generation: 1,
            capture: 9,
            callback: 8,
            tick: 7,
            offset: 0,
        };
        assert!(!c.select(CaptureRequest {
            connection: 39,
            ..request
        }));
        assert!(c.select(request));
        let view = c.view();
        assert!(view.valid());
        let page = view.detail.unwrap();
        assert_eq!(page.callback.budget_ns, 100_000);
        assert_eq!(page.nodes.len(), 3);
        assert_eq!(page.nodes[2].ready_ns, 110_000);
        assert_eq!(page.nodes[2].end_ns - page.nodes[2].start_ns, 10_000);
        assert!(!c.select(CaptureRequest {
            connection: 0,
            generation: 2,
            ..request
        }));
        assert!(!c.select(CaptureRequest {
            capture: 8,
            ..request
        }));
        callback.sequence = 10;
        callback.start_ns = tick.end_ns + 500;
        callback.end_ns = callback.start_ns + 100;
        c.receive(callback);
        assert!(!c.select(CaptureRequest {
            callback: 10,
            ..request
        }));
        c.status(Status {
            generation: 2,
            ..Status::default()
        });
        let view = c.view();
        assert!(view.detail.is_none() && view.index.is_empty());
    }

    #[shoop_test]
    fn invalid_tick_does_not_contribute_partial_statistics() {
        let mut c = Collector::default();
        c.status(Status {
            generation: 1,
            ..Status::default()
        });
        let mut p = prescribed(1, 20_000, 80_000);
        p.samples[3].key = 1;
        c.receive(p);
        assert!(c.view().live.rows.is_empty());
        p = prescribed(2, 20_000, 80_000);
        p.len = 2;
        c.receive(p);
        assert!(c.view().live.rows.is_empty());
    }

    #[shoop_test]
    fn missing_capture_chunks_mark_the_completed_interval_incomplete() {
        let mut c = Collector::default();
        c.status(Status {
            generation: 1,
            ..Status::default()
        });
        let mut p = prescribed(1, 20_000, 80_000);
        p.capture = 3;
        p.len = 1;
        c.receive(p);
        c.receive(Packet {
            generation: 1,
            capture: 3,
            sequence: 2,
            epoch: 4,
            start_ns: p.start_ns,
            end_ns: p.end_ns,
            flags: CALLBACK | CAPTURE_END,
            ..Packet::default()
        });
        let view = c.view();
        let capture = view.capture.unwrap();
        assert!(capture.complete && capture.incomplete);
        assert!(capture.rows.is_empty());
        assert_eq!(view.index.len(), 1);
    }

    #[shoop_test]
    fn capture_detail_pages_preserve_boundaries_without_copying_raw_history() {
        let mut c = Collector::default();
        c.status(Status {
            generation: 1,
            ..Status::default()
        });
        let samples: Vec<_> = core::iter::once(Sample {
            key: PHASE | Phase::GraphStart as u64,
            at: 100,
        })
        .chain((0..600).map(|key| Sample {
            key,
            at: 110 + key * 10,
        }))
        .collect();
        for (chunk, part) in samples.chunks(crate::CHUNK_SAMPLES).enumerate() {
            let mut p = Packet {
                generation: 1,
                capture: 1,
                sequence: 2,
                epoch: 4,
                start_ns: 100,
                end_ns: 6100,
                index: (chunk * crate::CHUNK_SAMPLES) as u32,
                len: part.len() as u8,
                total: samples.len() as u32,
                ..Packet::default()
            };
            p.samples[..part.len()].copy_from_slice(part);
            c.receive(p);
        }
        c.receive(Packet {
            generation: 1,
            capture: 1,
            sequence: 3,
            epoch: 4,
            start_ns: 90,
            end_ns: 6200,
            flags: CALLBACK | CAPTURE_END,
            budget_ns: 6000,
            ..Packet::default()
        });
        for (offset, expected) in [(0, 256), (256, 256), (512, 88)] {
            assert!(c.select(CaptureRequest {
                connection: 0,
                generation: 1,
                capture: 1,
                callback: 3,
                tick: 2,
                offset
            }));
            let page = c.view().detail.unwrap();
            assert_eq!(page.nodes.len(), expected);
            assert_eq!(page.total, 600);
            assert_eq!(page.nodes[0].key, u64::from(offset));
            assert_eq!(page.nodes[0].end_ns - page.nodes[0].start_ns, 10);
            assert_eq!(page.nodes[0].ready_ns, (u64::from(offset) + 1) * 10);
        }
        assert!(c.view().valid());
    }

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
