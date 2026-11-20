use crate::{
    clock::Edges,
    memory::axi::Axi,
    model::{Model, Probe},
    sim::Agent,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const ROOT: &str = "core.midi.service";
const PROBES: &[&str] = &[
    "core.midi.pending",
    "core.midi.service.command_valid",
    "core.midi.service.command_ready",
    "core.midi.service.command_op",
    "core.midi.service.op",
    "core.midi.service.phase",
    "core.midi.service.operation",
    "core.midi.service.micro_pc",
    "core.midi.service.state_pairs",
    "core.midi.service.states.phase",
    "core.midi.service.states.op",
];
pub fn probes() -> Vec<Probe> {
    PROBES.iter().map(|path| Probe::observe(path)).collect()
}

pub struct Profile {
    pub enabled: bool,
    started: bool,
    pub cycles: u64,
    categories: BTreeMap<&'static str, u64>,
    jobs: [u64; 64],
    job_counts: [u64; 64],
    phases: [u64; 64],
    instructions: [u64; 256],
    pcs: Vec<u64>,
    state_phases: [u64; 32],
    pairs: u64,
}
impl Default for Profile {
    fn default() -> Self {
        Self {
            enabled: false,
            started: false,
            cycles: 0,
            categories: BTreeMap::new(),
            jobs: [0; 64],
            job_counts: [0; 64],
            phases: [0; 64],
            instructions: [0; 256],
            pcs: vec![0; 16384],
            state_phases: [0; 32],
            pairs: 0,
        }
    }
}
impl Profile {
    pub fn begin(&mut self) {
        self.enabled = true;
        self.started = false;
        self.cycles = 0;
        self.categories.clear();
        self.jobs.fill(0);
        self.job_counts.fill(0);
        self.phases.fill(0);
        self.instructions.fill(0);
        self.pcs.fill(0);
        self.state_phases.fill(0);
        self.pairs = 0;
    }
    pub fn finish(&self, bus: &Axi, loops: usize) -> Value {
        let layout = super::MidiLayout::new(loops);
        let l = super::limits(loops).store;
        let table_refs = layout.table + l.views as usize * 35 * 4;
        let event_end = layout.event + l.blocks as usize * l.block_events as usize * 16;
        let contexts = event_end + l.retirements as usize * 8;
        let input = contexts + loops * 128;
        let activity = layout.output + super::DESTINATIONS * 16900;
        let history = activity + loops * 16900;
        let regions = [
            (0, "view_descriptors"),
            (layout.table, "event_block_tables"),
            (table_refs, "block_refs_and_free_stack"),
            (layout.snapshot, "snapshots"),
            (layout.event, "event_payloads"),
            (event_end, "retirements"),
            (contexts, "loop_contexts"),
            (input, "input_state"),
            (layout.output, "output_states"),
            (activity, "activity_states"),
            (history, "history_and_snapshot_info"),
            (layout.map, "comparison_bitmaps"),
            (layout.directory, "comparison_directory"),
            (
                layout.directory + l.snapshots as usize * super::DESTINATIONS * 48,
                "scratch_changes",
            ),
        ];
        let mut traffic = BTreeMap::<&str, [u64; 4]>::new();
        let mut previous = 0;
        for record in &bus.records {
            assert!(record.start > previous);
            previous = record.end;
            let offset = (record.address - bus.base) as usize;
            let name = regions
                .iter()
                .rev()
                .find(|(start, _)| offset >= *start)
                .unwrap()
                .1;
            let count = traffic.entry(name).or_default();
            count[usize::from(record.write)] += 1;
            count[2] += record.latency + 1;
            count[3] += (record.length + 1) << record.size;
        }
        let traffic: BTreeMap<_, _> = traffic
            .into_iter()
            .map(|(name, v)| {
                (
                    name,
                    json!({
                        "reads":v[0],"writes":v[1],"axi_occupied_cycles":v[2],"requested_bytes":v[3]
                    }),
                )
            })
            .collect();
        fn sparse(values: &[u64]) -> BTreeMap<usize, u64> {
            values
                .iter()
                .enumerate()
                .filter_map(|(i, &v)| (v != 0).then_some((i, v)))
                .collect()
        }
        assert_eq!(self.categories.values().sum::<u64>(), self.cycles);
        json!({"cycles":self.cycles,"categories":self.categories,"job_cycles":sparse(&self.jobs),
            "job_counts":sparse(&self.job_counts),"controller_phases":sparse(&self.phases),
            "executed_instructions":sparse(&self.instructions),"micro_pc_cycles":sparse(&self.pcs),
            "state_service_phases":sparse(&self.state_phases),"max_comparison_pairs":self.pairs,
            "memory_regions":traffic})
    }
}
impl Agent for Profile {
    fn sample(&mut self, model: &Model<'_>, edges: Edges) {
        if !self.enabled || !edges.rises(0) {
            return;
        }
        if !self.started {
            self.started = model.get("run_valid") != 0 && model.get("run_ready") != 0;
            if !self.started {
                return;
            }
        }
        self.cycles += 1;
        let get = |suffix: &str| model.get(&format!("{ROOT}.{suffix}"));
        let accepting = get("command_valid") != 0 && get("command_ready") != 0;
        let pending = model.get("core.midi.pending") != 0;
        let job = if accepting {
            get("command_op")
        } else {
            get("op")
        } as usize;
        if accepting {
            self.job_counts[job] += 1;
        }
        let phase = get("phase") as usize;
        self.phases[phase] += 1;
        self.pcs[get("micro_pc") as usize] += 1;
        if phase == 2 {
            self.instructions[get("operation") as usize] += 1;
        }
        let category = if !pending && !accepting {
            "sp_jp_and_job_admission"
        } else {
            self.jobs[job] += 1;
            match phase {
                9 => {
                    let p = get("states.phase") as usize;
                    self.state_phases[p] += 1;
                    self.pairs = self.pairs.max(get("state_pairs"));
                    match p {
                        3 | 4 => "tracker_directory_walk",
                        5 | 6 => "snapshot_validity_checks",
                        7 | 8 => "compare_changed_state_value",
                        10..=12 => {
                            if get("states.op") == 5 {
                                "send_recorded_packet"
                            } else {
                                "restore_or_cleanup"
                            }
                        }
                        14..=17 => "changed_value_bitmap",
                        _ => "apply_state_and_service_handshakes",
                    }
                }
                7 | 21 | 23 => "controller_memory_response_wait",
                11..=13 => "multiply_or_divide",
                16 | 17 | 25 | 26 => "loop_context_staging",
                27..=30 => "next_event_selection",
                31..=36 => "fast_job_paths",
                _ => "controller_instruction_execution",
            }
        };
        *self.categories.entry(category).or_default() += 1;
    }
}
