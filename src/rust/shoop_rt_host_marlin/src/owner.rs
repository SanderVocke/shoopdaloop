use super::bridge::{Block, Shared};
use anyhow::{ensure, Result};
use rt_fpga_compiler::{Identity, ProgramLimits};
use rt_fpga_reference::{
    fixed::Fixed,
    transitions::{LoopId, Mode},
};
use rt_fpga_rtl::{
    bits::Bits,
    host::control::{
        apply_iteration, apply_plan, clear_plan, descriptor, gain_value, loop_plan, loop_state,
        protocol_mode, reference_mode, set_repeat_sync,
    },
    host::{Upload, UploadStep},
    pipeline::{design, Geometry},
    sim::Sim,
    target::{ProgramImage, Space},
};
use shoop_rt_host_reference::{program_limits, StorageUsage, MEDIA_BLOCK_CHUNKS};
use shoop_rt_protocol::{
    codec,
    immediate::{Code, Command, CommandRecord, Header, SwitchProgram, TransitionMode, COMMANDS},
    snapshot::{self, Record},
    DspMeasurement, LoopSlotId, Peaks, SlotId, StateRecord, Telemetry, TimingState,
};
use shoop_rt_transport::rt;
use std::{
    sync::{atomic::Ordering, mpsc::SyncSender},
    thread,
    time::Duration,
};

const TARGET_CORE_CLOCK_HZ: u32 = 150_000_000;

struct StagedUpload {
    upload: Upload,
    done: bool,
    announced: bool,
}
impl StagedUpload {
    fn new(upload: Upload) -> Self {
        Self {
            upload,
            done: false,
            announced: false,
        }
    }
}
#[derive(Clone)]
struct Row {
    id: Identity,
    physical: Identity,
    gain: f32,
    peak: f32,
    failures: u32,
    last_pulse: Option<u64>,
}
struct Pending {
    header: Header,
    records: Vec<CommandRecord>,
    frame: u64,
}
struct Owner<'a> {
    sim: Sim<'a>,
    shared: &'a Shared,
    quantum: u32,
    chunk_frames: u32,
    frame: u64,
    storage_frame: u64,
    current: Option<ProgramImage>,
    upload: Option<StagedUpload>,
    retired: Option<ProgramImage>,
    rows: Vec<Row>,
    pending: Option<Pending>,
    snapshot: Vec<u8>,
}
impl Owner<'_> {
    fn tick(&mut self) -> Result<()> {
        ensure!(!self.shared.stop.load(Ordering::Acquire), "RTL stopped");
        if self.sim.model.get("core.unused_sync_event") != 0 {
            let event = self.sim.model.bits("core.unused_sync_data");
            let slot = event.get(32, 32) as u32;
            if let Some(row) = self.rows.iter_mut().find(|r| r.physical.slot == slot) {
                if event.get(0, 8) == 2 {
                    row.last_pulse = Some(event.get(192, 64));
                }
                if event.get(0, 8) == 3 && event.get(16, 8) == 3 {
                    row.failures = row.failures.saturating_add(1);
                }
            }
        }
        self.sim.idle(1);
        Ok(())
    }
    fn wait(&mut self, signal: &str, budget: usize) -> Result<()> {
        for _ in 0..budget {
            self.sim.model.eval();
            if self.sim.model.get(signal) != 0 {
                return Ok(());
            }
            self.tick()?;
        }
        anyhow::bail!(
            "RTL {signal} timed out after {budget} cycles, at frame {}, revision {}",
            self.frame,
            self.sim.model.get("active_revision")
        )
    }
    fn access(
        &mut self,
        prefix: &str,
        space: u64,
        address: u64,
        value: Option<&Bits>,
    ) -> Result<Bits> {
        let m = &mut self.sim.model;
        m.set(&format!("{prefix}_valid"), 1);
        m.set(&format!("{prefix}_write"), u64::from(value.is_some()));
        m.set(&format!("{prefix}_address"), address);
        if prefix == "cfg" {
            m.set_all(&[
                ("cfg_loop", u64::from(space == 2)),
                ("cfg_state", u64::from(space == 1)),
            ]);
        } else {
            m.set(&format!("{prefix}_space"), space);
        }
        if let Some(bits) = value {
            m.set_bits(&format!("{prefix}_write_data"), bits);
        }
        self.wait(&format!("{prefix}_ready"), 100_000)?;
        self.tick()?;
        self.sim.model.set(&format!("{prefix}_valid"), 0);
        self.wait(&format!("{prefix}_response_valid"), 100_000)?;
        ensure!(
            self.sim.model.get(&format!("{prefix}_response_error")) == 0,
            "RTL {prefix} rejected space {space}, address {address}"
        );
        let result = self.sim.model.bits(&format!("{prefix}_read_data"));
        self.tick()?;
        Ok(result)
    }
    fn write(&mut self, prefix: &str, space: u64, address: u64, value: u64) -> Result<()> {
        self.access(prefix, space, address, Some(&Bits::from_u64(value)))?;
        Ok(())
    }
    fn sample_storage(&mut self) {
        self.storage_frame = self.frame;
        // One configuration read latches block and page occupancy together.
        match self.access("store_cfg", 6, 16, None) {
            Ok(stats) => self.shared.status.publish_storage(StorageUsage {
                audio_blocks: (stats.get(0, 32), stats.get(32, 32)),
                metadata_pages: Some((stats.get(64, 32), stats.get(96, 32))),
            }),
            Err(_) => self.sim.model.set("store_cfg_valid", 0),
        }
    }
    fn clear_view(&mut self, view: u32) -> Result<()> {
        self.sim.model.set_all(&[
            ("store_cmd_valid", 1),
            ("store_cmd_opcode", 2),
            ("store_cmd_view", u64::from(view)),
            ("store_cmd_start", 0),
            ("store_cmd_length", 0),
            ("store_cmd_count", 0),
        ]);
        self.wait("store_cmd_ready", 1_000_000)?;
        self.tick()?;
        self.sim.model.set("store_cmd_valid", 0);
        self.wait("store_done", 1_000_000)?;
        ensure!(
            self.sim.model.get("store_error_code") == 0,
            "RTL view reclamation failed"
        );
        self.tick()?;
        Ok(())
    }
    fn upload_turn(&mut self, ready: &mut rtrb::Producer<(u64, bool)>) -> Result<bool> {
        let Some(mut upload) = self.upload.take() else {
            return Ok(false);
        };
        if upload.announced {
            self.upload = Some(upload);
            return Ok(false);
        }
        if !upload.done {
            for _ in 0..32 {
                match upload.upload.next_step() {
                    Some(UploadStep::Configure(w)) => {
                        let (prefix, space) = if w.space == Space::History {
                            ("audio_cfg", 3)
                        } else {
                            ("cfg", w.space as u64)
                        };
                        self.access(prefix, space, w.address, Some(&w.value))?;
                    }
                    Some(UploadStep::ClearView(view)) => {
                        self.clear_view(view)?;
                        break;
                    }
                    None => {
                        upload.done = true;
                        break;
                    }
                }
            }
        }
        if upload.done {
            if ready
                .push((upload.upload.image.layout.revision, true))
                .is_ok()
            {
                // A ready image stays owned here until its fenced switch.
                upload.announced = true;
                self.upload = Some(upload);
                return Ok(false);
            }
        }
        self.upload = Some(upload);
        Ok(true)
    }
    fn select(&mut self) -> Result<()> {
        let revision = self.sim.model.get("active_revision");
        if revision == self.current.as_ref().map_or(0, |p| p.layout.revision) {
            return Ok(());
        }
        let upload = self
            .upload
            .take()
            .ok_or_else(|| anyhow::anyhow!("RTL selected an unstaged program"))?
            .upload;
        ensure!(
            upload.image.layout.revision == revision,
            "RTL selected wrong program revision"
        );
        let mut rows = Vec::new();
        for binding in &upload.image.compiled.bindings {
            let allocation = upload
                .image
                .layout
                .loops
                .iter()
                .find(|l| l.physical == binding.id)
                .ok_or_else(|| anyhow::anyhow!("missing RTL binding"))?;
            let row = self
                .rows
                .iter()
                .find(|r| r.id == allocation.logical && r.physical == allocation.physical)
                .cloned()
                .unwrap_or_else(|| Row {
                    id: allocation.logical,
                    physical: allocation.physical,
                    gain: binding.gain.map_or(1., |a| {
                        Fixed(upload.image.compiled.state[a as usize].get(32, 32) as i32).to_f32()
                    }),
                    peak: 0.,
                    failures: 0,
                    last_pulse: None,
                });
            rows.push(row);
        }
        rows.sort_by_key(|r| r.id);
        self.rows = rows;
        ensure!(self.retired.is_none(), "RTL retirement storage full");
        self.retired = self.current.replace(upload.image);
        self.sim.model.set("switch_valid", 0);
        Ok(())
    }
    fn loop_words(&mut self, id: Identity) -> Result<[u64; 16]> {
        let mut words = [0; 16];
        for (i, w) in words.iter_mut().enumerate() {
            *w = self
                .access("cfg", 2, u64::from(id.slot) * 16 + i as u64, None)?
                .get(0, 64);
        }
        Ok(words)
    }
    fn store_loop(&mut self, id: Identity, words: &[u64; 16]) -> Result<()> {
        for (i, w) in words.iter().enumerate() {
            self.write("cfg", 2, u64::from(id.slot) * 16 + i as u64, *w)?;
        }
        Ok(())
    }
    fn execute(&mut self, command: Command) -> Result<Code> {
        if !command.valid() {
            return Ok(Code::InvalidValue);
        }
        let target = match command {
            Command::Transition(id, _)
            | Command::ClearPlannedTransitions(id, _)
            | Command::LoopGain(id, _) => id,
            Command::LoopIteration(id, _) | Command::LoopRepeatSync(id, _) => id.0.index,
            _ => return Ok(Code::Unsupported),
        };
        let Some(index) = self.rows.iter().position(|r| r.id.slot == target) else {
            return Ok(Code::TargetUnavailable);
        };
        let physical = self.rows[index].physical;
        if let Command::LoopGain(_, gain) = command {
            let Some(address) = self
                .current
                .as_ref()
                .and_then(|c| c.compiled.bindings.iter().find(|b| b.id == physical))
                .and_then(|b| b.gain)
            else {
                return Ok(Code::Unsupported);
            };
            let bits = gain_value(gain);
            self.access("cfg", 1, u64::from(address), Some(&bits))?;
            self.rows[index].gain = gain;
            return Ok(Code::Applied);
        }
        let mut words = self.loop_words(physical)?;
        if let Command::ClearPlannedTransitions(_, mask) = command {
            clear_plan(&mut words, mask);
            self.store_loop(physical, &words)?;
            return Ok(Code::Applied);
        }
        let state = loop_state(physical, &words, self.rows[index].last_pulse);
        let control = match command {
            Command::LoopIteration(id, request)
                if id.0.generation == self.rows[index].id.generation =>
            {
                apply_iteration(&mut words, state, self.frame, request)
            }
            Command::LoopRepeatSync(id, enabled)
                if id.0.generation == self.rows[index].id.generation =>
            {
                set_repeat_sync(&mut words, enabled);
                Ok(Code::Applied)
            }
            Command::LoopIteration(..) | Command::LoopRepeatSync(..) => {
                Err(Code::TargetUnavailable)
            }
            Command::Transition(_, plan) => {
                let image = self.current.as_ref().unwrap();
                let source = image
                    .layout
                    .loops
                    .iter()
                    .find(|l| l.physical == physical)
                    .and_then(|l| l.sync_source)
                    .map(|logical| image.layout.loop_id(logical))
                    .transpose()?
                    .map(|id| LoopId {
                        slot: id.slot,
                        generation: id.generation,
                    });
                apply_plan(&mut words, state, source, self.frame, plan)
            }
            _ => unreachable!(),
        };
        match control {
            Ok(code) => {
                self.store_loop(physical, &words)?;
                Ok(code)
            }
            Err(code) => Ok(code),
        }
    }
    fn command_turn(&mut self, execution: &mut rt::Execution) -> Result<bool> {
        execution.commands.begin_callback();
        if self.pending.is_some() {
            if self.sim.model.get("switch_response_valid") == 0 {
                return Ok(false);
            }
            let pending = self.pending.take().unwrap();
            ensure!(
                self.sim.model.get("switch_response_error") == 0,
                "RTL ownership fence failed"
            );
            let group = execution
                .commands
                .pop()
                .ok_or_else(|| anyhow::anyhow!("missing pending command group"))?;
            let mut i = 0;
            group.respond(
                pending.header.complete(pending.frame, Code::Applied),
                |_| {
                    let r = pending.records[i];
                    i += 1;
                    r
                },
            );
            self.sim.model.set("switch_response_ready", 1);
            self.tick()?;
            self.sim.model.set("switch_response_ready", 0);
            return Ok(true);
        }
        let Some(group) = execution.commands.pop() else {
            return Ok(false);
        };
        let header = group.header;
        let records = group.commands().copied().collect::<Vec<_>>();
        if self.retired.is_some()
            && records
                .iter()
                .any(|r| matches!(r.command(), Command::SwitchProgram(_)))
        {
            group.leave_unread();
            return Ok(false);
        }
        let code = if (header.graph(), header.topology())
            != (
                self.sim.model.get("active_revision"),
                self.sim.model.get("active_revision"),
            ) {
            Code::StaleConfiguration
        } else if header.deadline().is_some_and(|f| self.frame > f) {
            Code::DeadlineMissed
        } else if records.iter().enumerate().any(|(i, r)| {
            matches!(r.command(), Command::SwitchProgram(_)) && i + 1 != records.len()
        }) {
            Code::CannotApply
        } else {
            Code::Applied
        };
        if code != Code::Applied {
            for record in &records {
                if let Command::SwitchProgram(next) = record.command() {
                    self.retire_refused_upload(next);
                }
            }
            group.respond(header.complete(self.frame, code), |r| r.complete(code));
            return Ok(true);
        }
        if let [record] = records.as_slice() {
            if let Command::SwitchProgram(descriptor) = record.command() {
                if self.waits_for_composite_cut(descriptor)? {
                    group.leave_unread();
                    return Ok(false);
                }
            }
        }
        let mut complete = Vec::with_capacity(COMMANDS);
        let mut switching = false;
        for record in &records {
            let code = if let Command::SwitchProgram(d) = record.command() {
                if self.retired.is_some()
                    || !self
                        .upload
                        .as_ref()
                        .is_some_and(|u| u.announced && descriptor(&u.upload.image) == d)
                {
                    Code::CannotApply
                } else if !self.composite_cuts_ready()? {
                    Code::CannotApply
                } else {
                    let changed = self
                        .upload
                        .as_ref()
                        .unwrap()
                        .upload
                        .image
                        .layout
                        .changed_sources()
                        .to_vec();
                    for loop_id in changed {
                        let code = self.execute(Command::ClearPlannedTransitions(
                            loop_id.slot,
                            shoop_rt_protocol::immediate::ClearPlanned::Both,
                        ))?;
                        ensure!(
                            code == Code::Applied,
                            "RTL could not clear the continuing loop plan"
                        );
                    }
                    self.sim.model.set_all(&[
                        ("switch_valid", 1),
                        ("switch_expected", header.graph()),
                        ("switch_revision", d.revision),
                        ("switch_region", u64::from(d.region)),
                        ("switch_instructions", u64::from(d.instructions)),
                        ("switch_sync_count", u64::from(d.sync_count)),
                        ("switch_states", u64::from(d.states)),
                        ("switch_jobs", u64::from(d.jobs)),
                        ("switch_buffers", u64::from(d.buffers)),
                    ]);
                    switching = true;
                    Code::Applied
                }
            } else {
                self.execute(record.command())?
            };
            if code != Code::Applied {
                if let Command::SwitchProgram(next) = record.command() {
                    self.retire_refused_upload(next);
                }
            }
            complete.push(record.complete(code));
        }
        if switching {
            self.pending = Some(Pending {
                header,
                records: complete,
                frame: self.frame,
            });
            group.leave_unread();
        } else {
            let mut i = 0;
            group.respond(header.complete(self.frame, Code::Applied), |_| {
                let r = complete[i];
                i += 1;
                r
            });
        }
        Ok(true)
    }

    fn retire_refused_upload(&mut self, requested: SwitchProgram) {
        if self
            .upload
            .as_ref()
            .is_some_and(|upload| upload.announced && descriptor(&upload.upload.image) == requested)
        {
            self.retired = self.upload.take().map(|upload| upload.upload.image);
        }
    }

    fn waits_for_composite_cut(&mut self, switch: SwitchProgram) -> Result<bool> {
        let Some(upload) = self.upload.as_ref() else {
            return Ok(false);
        };
        if self.retired.is_some() || !upload.announced || descriptor(&upload.upload.image) != switch
        {
            return Ok(false);
        }
        let [cut] = upload.upload.image.layout.composite_cuts() else {
            return Ok(false);
        };
        let cut = *cut;
        if !cut.allow_restart {
            return Ok(false);
        }
        let words = self.loop_words(cut.old_slot)?;
        Ok(rt_fpga_rtl::host::control::composite_cut_waits_for_restart(
            cut, &words,
        ))
    }

    fn composite_cuts_ready(&mut self) -> Result<bool> {
        let cuts = self
            .upload
            .as_ref()
            .unwrap()
            .upload
            .image
            .layout
            .composite_cuts()
            .to_vec();
        for cut in cuts {
            let words = self.loop_words(cut.old_slot)?;
            if !rt_fpga_rtl::host::control::composite_cut_ready(cut, &words) {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn publish(&mut self, execution: &mut rt::Execution, processed: bool) -> Result<()> {
        let mut bytes = std::mem::take(&mut self.snapshot);
        let mut position = snapshot::HEADER;
        let mut records = 0;
        let frame = self.frame.saturating_sub(if processed {
            u64::from(self.quantum)
        } else {
            0
        });
        for index in 0..self.rows.len() {
            let row = self.rows[index].clone();
            let words = self.loop_words(row.physical)?;
            let state = loop_state(row.physical, &words, row.last_pulse);
            let plan = loop_plan(&words);
            let id = LoopSlotId(SlotId::new(row.id.slot, row.id.generation));
            let next = [plan.first, plan.second]
                .into_iter()
                .flatten()
                .min_by_key(|s| s.skip);
            let armed = [plan.first, plan.second]
                .into_iter()
                .flatten()
                .any(|s| s.mode == TransitionMode::Recording);
            let timing = TimingState {
                content_revision: state.content_revision,
                length: state.content_length,
                raw_audio_frames: state.content_length,
                raw_midi_frames: state.content_length,
                ..Default::default()
            };
            let binding = self
                .current
                .as_ref()
                .unwrap()
                .compiled
                .bindings
                .iter()
                .find(|b| b.id == row.physical)
                .unwrap();
            let meter = binding.meter;
            for record in [
                Record::LoopPlan {
                    id,
                    plan,
                    failures: row.failures,
                },
                Record::LoopLevels {
                    id,
                    gain: row.gain,
                    balance: 0.,
                    repeat_sync: state.repeat_sync,
                },
                Record::State(StateRecord::LoopTiming {
                    loop_id: id,
                    timing,
                }),
                Record::Telemetry(Telemetry::Loop {
                    loop_id: id,
                    mode: protocol_mode(state.mode),
                    next_mode: next.map(|s| protocol_mode(reference_mode(s.mode))),
                    next_delay: next.map(|s| s.skip),
                    content_revision: state.content_revision,
                    recording_armed: armed,
                    capture_incomplete: false,
                    timing_revision: timing.revision,
                    length: if state.mode == Mode::Recording {
                        state.capture_frames.saturating_sub(if processed {
                            u64::from(self.quantum)
                        } else {
                            0
                        })
                    } else {
                        state.content_length
                    },
                    position: if processed { words[8] } else { state.position },
                    peaks: if meter.is_some() {
                        Peaks::new(&[row.peak]).unwrap()
                    } else {
                        Peaks::default()
                    },
                }),
            ] {
                position += codec::encode_snapshot(&record, &mut bytes[position..])
                    .map_err(|e| anyhow::anyhow!("snapshot encode {e:?}"))?;
                records += 1;
            }
            if let Some(composite) = self
                .current
                .as_ref()
                .unwrap()
                .layout
                .composite_schedule(row.id)
            {
                let pending = self
                    .upload
                    .as_ref()
                    .and_then(|staged| staged.upload.image.layout.composite_schedule(row.id))
                    .filter(|plan| plan.version != composite.version);
                let playhead = if processed { words[8] } else { state.position };
                let unit = u64::from(composite.unit_frames);
                let record = Record::Telemetry(Telemetry::Composite {
                    loop_id: id,
                    frame,
                    mode: protocol_mode(state.mode),
                    position: playhead,
                    length: state.content_length,
                    active_version: composite.version,
                    pending_version: pending.map(|plan| plan.version),
                    pending_admitted: pending.is_some()
                        && self.upload.as_ref().is_some_and(|staged| staged.announced),
                    iteration: (playhead / unit)
                        .min((state.content_length / unit).saturating_sub(1)),
                    passes: state.cycles,
                    waiting_repeat: words[1] & (1 << 42) != 0,
                });
                position += codec::encode_snapshot(&record, &mut bytes[position..])
                    .map_err(|e| anyhow::anyhow!("snapshot encode {e:?}"))?;
                records += 1;
            }
            if processed {
                let mut peak = 0f32;
                if let Some(buffer) = meter {
                    for i in 0..self.quantum {
                        let raw = self
                            .access(
                                "audio_cfg",
                                2,
                                u64::from(buffer) * u64::from(self.quantum) + u64::from(i),
                                None,
                            )?
                            .get(0, 32) as i32;
                        peak = peak.max(Fixed(raw).to_f32().abs());
                    }
                }
                self.rows[index].peak = peak;
            }
        }
        let revision = self.sim.model.get("active_revision");
        snapshot::Header {
            frame,
            graph: revision,
            topology: revision,
            records,
            loops: self.rows.len() as u32,
            strips: 0,
            buses: 0,
        }
        .write(&mut bytes);
        let _ = execution.snapshots.publish(&bytes[..position]);
        self.snapshot = bytes;
        Ok(())
    }
    fn quantum(&mut self, block: &mut Block, execution: &mut rt::Execution) -> Result<()> {
        ensure!(
            block.frame == self.frame,
            "RTL render demand is out of order"
        );
        for i in 0..self.quantum {
            self.write(
                "audio_cfg",
                0,
                u64::from(i),
                Fixed::from_f32(block.input[i as usize]).0 as u32 as u64,
            )?;
        }
        self.sim.model.set_all(&[
            ("run_valid", 1),
            ("run_instruction_count", 0),
            ("run_state_count", u64::from(program_limits().state_values)),
            ("run_quantum", u64::from(self.quantum)),
            ("run_chunk_frames", u64::from(self.chunk_frames)),
            ("run_job_budget", u64::from(program_limits().jobs)),
            ("run_buffers", u64::from(program_limits().buffers)),
            ("run_inputs", 1),
            ("run_outputs", 2),
        ]);
        let mut accepted_at = None;
        for _ in 0..2_000_000 {
            if self.pending.is_some() && self.sim.model.get("switch_response_valid") != 0 {
                self.sim.model.set("run_valid", 0);
                self.command_turn(execution)?;
                for _ in 0..128 {
                    if !self.command_turn(execution)? {
                        break;
                    }
                }
                self.sim.model.set("run_valid", 1);
            }
            self.sim.model.eval();
            let ready =
                self.sim.model.get("run_valid") != 0 && self.sim.model.get("run_ready") != 0;
            let revision = self.sim.model.get("active_revision");
            self.tick()?;
            self.select()?;
            if self.sim.model.get("active_revision") != revision {
                self.sim.model.set("run_valid", 0);
            }
            if ready {
                accepted_at = Some(self.sim.cycles);
                break;
            }
        }
        let accepted_at =
            accepted_at.ok_or_else(|| anyhow::anyhow!("RTL quantum admission timed out"))?;
        self.sim.model.set("run_valid", 0);
        self.wait("done", 20_000_000)?;
        let cycles = self.sim.cycles - accepted_at;
        ensure!(
            self.sim.model.get("error_code") == 0,
            "RTL quantum failed: stage {}, code {}",
            self.sim.model.get("error_stage"),
            self.sim.model.get("error_code")
        );
        self.tick()?;
        self.frame += u64::from(self.quantum);
        ensure!(
            self.sim.model.get("sync_frame") == self.frame,
            "RTL logical frame mismatch"
        );
        for port in 0..2 {
            for i in 0..self.quantum {
                block.output[port][i as usize] = Fixed(
                    self.access(
                        "audio_cfg",
                        1,
                        port as u64 * u64::from(self.quantum) + u64::from(i),
                        None,
                    )?
                    .get(0, 32) as i32,
                )
                .to_f32();
            }
        }
        self.publish(execution, true)?;
        if self.frame.saturating_sub(self.storage_frame) >= self.shared.status.usage_interval() {
            self.sample_storage();
        }
        execution
            .commands
            .publish_frame(rt_runtime::Frame(self.frame));
        self.shared.status.record_dsp(DspMeasurement::FpgaQuantum {
            cycles,
            clock_hz: TARGET_CORE_CLOCK_HZ,
            frames: self.quantum,
            sample_rate: self.shared.status.driver().sample_rate,
        });
        self.shared.status.set_frame(self.frame);
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    limits: ProgramLimits,
    chunk_frames: u32,
    memory_mb: u32,
    shared: &Shared,
    mut execution: rt::Execution,
    mut uploads: rtrb::Consumer<Upload>,
    mut ready: rtrb::Producer<(u64, bool)>,
    mut retired: rtrb::Producer<ProgramImage>,
    mut requests: rtrb::Consumer<Block>,
    mut results: rtrb::Producer<Block>,
    started: SyncSender<std::result::Result<(), String>>,
) -> Result<()> {
    let frames = u64::from(memory_mb) * 1024 * 1024 / 4;
    let frames_per_chunk = u64::from(chunk_frames);
    // Keep the sample capacity and allocation block size in frames constant.
    let geometry = Geometry::new(
        [
            limits.state_values.into(),
            frames / frames_per_chunk,
            frames_per_chunk,
        ],
        MEDIA_BLOCK_CHUNKS as u64 / frames_per_chunk,
    );
    let built = design(&geometry, limits).build(&rt_fpga_rtl::root().join("work/marlin-host"));
    let compiled = match built {
        Ok(c) => c,
        Err(e) => {
            let message = e.to_string();
            let _ = started.send(Err(message.clone()));
            anyhow::bail!(message);
        }
    };
    let model = match compiled.session(None) {
        Ok(m) => m,
        Err(e) => {
            let message = e.to_string();
            let _ = started.send(Err(message.clone()));
            anyhow::bail!(message);
        }
    };
    let clock_period_ps = 1_000_000_000_000 / u64::from(TARGET_CORE_CLOCK_HZ);
    let mut sim = Sim::new(model, &[("clk", clock_period_ps, 0)]);
    sim.max_ps = u64::MAX;
    let mut owner = Owner {
        sim,
        shared,
        quantum: limits.quantum,
        chunk_frames,
        frame: 0,
        storage_frame: 0,
        current: None,
        upload: None,
        retired: None,
        rows: Vec::new(),
        pending: None,
        snapshot: vec![0; snapshot::capacity(0, limits.loop_slots as usize, 0).unwrap()],
    };
    let init = (|| -> Result<()> {
        owner.sim.model.set_all(&[
            ("reset", 1),
            ("job_enable", 1),
            ("arena_base", 0x1234000000),
            ("maintenance_enable", 1),
        ]);
        owner.tick()?;
        owner.tick()?;
        owner.sim.model.set("reset", 0);
        owner.tick()?;
        for view in 0..geometry.views {
            owner.write("store_cfg", 1, view, 1)?;
        }
        for (address, value) in [(0, geometry.views), (1, 0), (2, 0), (3, geometry.blocks())] {
            owner.write("store_cfg", 0, address, value)?;
        }
        owner.sample_storage();
        owner.publish(&mut execution, false)?;
        Ok(())
    })();
    if let Err(e) = init {
        let message = e.to_string();
        let _ = started.send(Err(message.clone()));
        anyhow::bail!(message);
    }
    let _ = started.send(Ok(()));
    let mut output = None;
    while !shared.stop.load(Ordering::Acquire) {
        let mut work = false;
        if let Some(image) = owner.retired.take() {
            if owner.pending.is_some() {
                owner.retired = Some(image);
            } else if let Err(rtrb::PushError::Full(image)) = retired.push(image) {
                owner.retired = Some(image);
            } else {
                work = true;
            }
        }
        if owner.upload.is_none() {
            if let Ok(upload) = uploads.pop() {
                owner.upload = Some(StagedUpload::new(upload));
                work = true;
            }
        }
        work |= owner.upload_turn(&mut ready)?;
        work |= owner.command_turn(&mut execution)?;
        if let Some(block) = output.take() {
            if let Err(rtrb::PushError::Full(block)) = results.push(block) {
                output = Some(block);
            } else {
                work = true;
            }
        }
        if output.is_none() {
            if let Ok(mut block) = requests.pop() {
                owner.command_turn(&mut execution)?;
                owner.quantum(&mut block, &mut execution)?;
                output = Some(block);
                work = true;
            }
        }
        if !work {
            thread::park_timeout(Duration::from_millis(1));
        }
    }
    Ok(())
}
