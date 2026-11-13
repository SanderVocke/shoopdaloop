//! Autonomous coordinator, processor, allocator and physical DDR measurements.
use crate::{
    bits::Bits,
    clock::Edges,
    codec,
    memory::Shared,
    model::{Model, Probe},
    sim::Agent,
    timing::dram::Snapshot,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
const RECLAIM: &[&str] = &[
    "pending",
    "references",
    "active",
    "high_water",
    "blocks",
    "freed",
    "cycles",
    "pages_used",
    "pages_available",
    "pages_freed",
    "capacity_failures",
];
fn labels(file: &str, kind: &str) -> Vec<String> {
    let text = std::fs::read_to_string(crate::root().join(file)).unwrap();
    let before = text.split(&format!("}} {kind};")).next().unwrap();
    before
        .rsplit_once('{')
        .unwrap()
        .1
        .split(',')
        .map(|s| s.trim().to_owned())
        .collect()
}
fn metric_paths(axi: bool) -> Vec<(String, String)> {
    let core = if axi { "core." } else { "" };
    let mut paths = Vec::new();
    if axi {
        for (i, client) in ["sample", "metadata"].iter().enumerate() {
            for metric in ["reads", "writes", "bytes", "wait_cycles"] {
                paths.push((
                    format!("{client}_{metric}"),
                    format!("memory_endpoint.scheduler.{metric}[{i}]"),
                ));
            }
        }
    }
    for (i, bank) in ["table", "reference", "stack"].iter().enumerate() {
        for metric in ["hits", "misses", "fills", "writebacks"] {
            paths.push((
                format!("{bank}_{metric}"),
                format!("{core}audio.media.metadata.banks[{i}].cache.{metric}"),
            ));
        }
    }
    for metric in ["directory_reads", "table_reads"] {
        paths.push((
            metric.into(),
            format!("{core}audio.media.metadata.{metric}"),
        ));
    }
    for (i, metric) in [
        "table_read",
        "table_write",
        "reference_read",
        "reference_write",
        "stack_read",
        "stack_write",
        "trim",
        "capacity_check",
        "fence",
        "invalidate",
    ]
    .iter()
    .enumerate()
    {
        paths.push((
            format!("{metric}_operations"),
            format!("{core}audio.media.metadata.operations[{i}]"),
        ));
    }
    paths
}
pub fn probes(axi: bool) -> Vec<Probe> {
    let core = if axi { "core." } else { "" };
    let mut probes: Vec<_> = [
        "phase",
        "timing.state",
        "audio.pipeline.sample_engine.phase",
        "audio.media.sample_memory.phase",
        "audio.media.sample_memory.opcode",
        "audio.media.sample_memory.dma_phase",
        "audio.media.advance",
        "audio.media.metadata.phase",
    ]
    .iter()
    .map(|n| Probe::observe(&format!("{core}{n}")))
    .collect();
    probes.extend(
        metric_paths(axi)
            .iter()
            .map(|(_, path)| Probe::observe(path)),
    );
    probes
}
#[derive(Default)]
struct Window {
    start: Option<u64>,
    total: Option<u64>,
    phases: BTreeMap<String, u64>,
    jp: BTreeMap<String, u64>,
    tp: BTreeMap<String, u64>,
    completion: BTreeMap<String, u64>,
    jobs: Vec<(Value, u64, Option<u64>)>,
    state: u64,
    job: u64,
    fill: u64,
    dma: u64,
    metadata_stalls: u64,
    metadata_busy: u64,
    metadata_start: BTreeMap<String, u64>,
    metadata_end: BTreeMap<String, u64>,
    reclaim_start: BTreeMap<String, u64>,
    reclaim_end: BTreeMap<String, u64>,
    requests_start: (u64, u64),
    requests_end: (u64, u64),
    bus_start: Option<Snapshot>,
    bus_end: Option<Snapshot>,
}
pub struct Execution {
    memory: Shared,
    axi: bool,
    core: String,
    cycle: u64,
    enabled: bool,
    coordinator: Vec<String>,
    jobs: Vec<String>,
    transition: Vec<String>,
    metrics: Vec<(String, String)>,
    window: Window,
}
impl Execution {
    pub fn new(memory: Shared, axi: bool) -> Self {
        Self {
            memory,
            axi,
            core: if axi { "core." } else { "" }.into(),
            cycle: 0,
            enabled: false,
            coordinator: labels("fpga/loop_pipeline/rtl/loop_pipeline.sv", "phase_t"),
            jobs: labels("fpga/job_processor/rtl/job_processor.sv", "phase_t"),
            transition: labels(
                "fpga/transition_processor/rtl/transition_processor.sv",
                "state_t",
            ),
            metrics: metric_paths(axi),
            window: Window::default(),
        }
    }
    pub fn begin(&mut self) {
        assert!(!self.enabled);
        self.window = Window::default();
        self.enabled = true;
    }
    fn metadata(&self, m: &Model<'_>) -> BTreeMap<String, u64> {
        self.metrics
            .iter()
            .map(|(label, path)| (label.clone(), m.get(path)))
            .collect()
    }
    fn reclaim(&self, m: &Model<'_>) -> BTreeMap<String, u64> {
        RECLAIM
            .iter()
            .map(|n| ((*n).into(), m.get(&format!("reclaim_{n}"))))
            .collect()
    }
    pub fn finish(&self) -> Value {
        assert!(!self.enabled);
        let w = &self.window;
        let total = w.total.expect("measurement missed coordinator completion");
        assert_eq!(w.phases.values().sum::<u64>(), total);
        let mut jobs = BTreeMap::<String, (u64, u64)>::new();
        for (job, start, end) in &w.jobs {
            let group = jobs
                .entry(job["kind"].as_str().unwrap().into())
                .or_default();
            group.0 += 1;
            group.1 += end.expect("job interval has no completion") - start;
        }
        let jobs: BTreeMap<_, _> = jobs
            .into_iter()
            .map(|(kind, (count, cycles))| (kind, json!({"count":count,"cycles":cycles})))
            .collect();
        let tp: u64 = w
            .phases
            .iter()
            .filter(|(name, _)| name.starts_with("SETTLE_") || name.starts_with("FINISH_"))
            .map(|(_, n)| *n)
            .sum();
        let mut metadata: BTreeMap<_, _> = w
            .metadata_end
            .iter()
            .map(|(k, v)| (k.clone(), v - w.metadata_start[k]))
            .collect();
        for access in ["read", "write"] {
            let bytes: u64 = ["table", "reference", "stack"]
                .iter()
                .map(|bank| metadata[&format!("{bank}_{access}_operations")])
                .sum::<u64>()
                * 4;
            metadata.insert(format!("logical_{access}_bytes"), bytes);
        }
        metadata.insert(
            "directory_read_bytes".into(),
            4 * metadata["directory_reads"],
        );
        let reclaim: BTreeMap<_, _> = [
            "blocks",
            "freed",
            "cycles",
            "pages_freed",
            "capacity_failures",
        ]
        .iter()
        .map(|&n| (n, w.reclaim_end[n] - w.reclaim_start[n]))
        .collect();
        let bus = w
            .bus_start
            .zip(w.bus_end)
            .and_then(|(a, b)| self.memory.borrow().bus_utilization(a, b));
        json!({"completion_cycles":w.completion,"metadata_statistics":metadata,"reclamation":{"start":w.reclaim_start,"end":w.reclaim_end,"work":reclaim},"raw_cycles":total,"active_cycles":total,"axi_reads":w.requests_end.0-w.requests_start.0,"axi_writes":w.requests_end.1-w.requests_start.1,"transition_cycles":tp,"state_cycles":w.state,"job_cycles":w.job,"other_cycles":total-tp-w.state-w.job,"coordinator_phases":w.phases,"job_phases":w.jp,"transition_states":w.tp,"jobs":jobs,"demand_fill_wait_cycles":w.fill,"transfer_active_cycles":w.dma,"metadata_foreground_stall_cycles":w.metadata_stalls,"metadata_service_busy_cycles":w.metadata_busy,"raw_ddr_bus":bus,"active_ddr_bus":bus,"axi":self.axi})
    }
}
impl Agent for Execution {
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if !e.rises(0) {
            return;
        }
        let cycle = self.cycle;
        self.cycle += 1;
        if !self.enabled {
            return;
        }
        if self.window.start.is_none() {
            if m.get("run_valid") != 0 && m.get("run_ready") != 0 {
                let metadata = self.metadata(m);
                let reclaim = self.reclaim(m);
                let memory = self.memory.borrow();
                let w = &mut self.window;
                w.start = Some(cycle + 1);
                w.metadata_start = metadata;
                w.reclaim_start = reclaim;
                w.requests_start = (memory.reads(), memory.writes());
                w.bus_start = memory.bus_snapshot();
            }
            return;
        }
        let time = cycle - self.window.start.unwrap();
        let get = |name: &str| m.get(&format!("{}{name}", self.core));
        let phase = self.coordinator[get("phase") as usize].clone();
        for signal in ["state_done", "jobs_done"] {
            if m.get(signal) != 0 {
                self.window.completion.entry(signal.into()).or_insert(time);
            }
        }
        if phase == "AUDIO_START" {
            self.window
                .completion
                .entry("transition_settle_done".into())
                .or_insert(time);
        }
        if m.get("done") != 0 {
            let metadata = self.metadata(m);
            let reclaim = self.reclaim(m);
            let memory = self.memory.borrow();
            let w = &mut self.window;
            w.total = Some(time);
            w.completion.insert("finish_done".into(), time);
            w.metadata_end = metadata;
            w.reclaim_end = reclaim;
            w.requests_end = (memory.reads(), memory.writes());
            w.bus_end = memory.bus_snapshot();
            self.enabled = false;
            return;
        }
        let w = &mut self.window;
        *w.phases.entry(phase.clone()).or_default() += 1;
        if matches!(
            phase.as_str(),
            "SETTLE_SEND" | "SETTLE_WAIT" | "FINISH_SEND" | "FINISH_WAIT"
        ) {
            *w.tp
                .entry(self.transition[get("timing.state") as usize].clone())
                .or_default() += 1;
        }
        w.state += u64::from(m.get("state_busy") != 0);
        if m.get("jobs_busy") != 0 {
            w.job += 1;
            *w.jp
                .entry(self.jobs[get("audio.pipeline.sample_engine.phase") as usize].clone())
                .or_default() += 1;
        }
        if m.get("monitor_valid") != 0 && m.get("monitor_ready") != 0 {
            if let Some((_, _, end)) = w.jobs.last_mut() {
                *end = Some(time);
            }
            let packet: Bits = m.bits("monitor_data");
            w.jobs.push((codec::job(&packet), time, None));
        }
        if m.get("jobs_done") != 0
            && let Some((_, _, end)) = w.jobs.last_mut()
        {
            end.get_or_insert(time);
        }
        w.fill += u64::from(
            get("audio.media.sample_memory.phase") == 3
                && get("audio.media.sample_memory.opcode") == 0,
        );
        w.dma += u64::from(get("audio.media.sample_memory.dma_phase") != 0);
        w.metadata_stalls += u64::from(get("audio.media.advance") == 0);
        w.metadata_busy += u64::from(get("audio.media.metadata.phase") != 0);
    }
}
