use super::{image_channel, ImageOwner, Images};
use crate::{prepare_control, program_limits, Endpoint, Status, StorageUsage, MEDIA_BLOCK_CHUNKS};
use alloc::{sync::Arc, vec, vec::Vec};
use anyhow::{ensure, Result};
use core::fmt::Debug;
use rt_fpga_compiler::{Identity, ProgramLimits};
use rt_fpga_reference::{
    fixed::Fixed,
    transitions::{LoopId, Mode},
};
use rt_fpga_rtl::{
    host::{
        control::{
            composite_telemetry, descriptor, loop_plan, loop_state, protocol_mode, reference_mode,
            ControlWindow, ProgramSession,
        },
        registers::{HOST_PAYLOAD_WORDS, RESULT_OK},
        Client, Error, Geometry, Mailbox, RegisterBus, Request,
    },
    target::ProgramImage,
};
use shoop_rt_protocol::{
    codec,
    immediate::{
        ClearPlanned, Code, Command, CommandRecord, SwitchProgram, TransitionMode, COMMANDS,
    },
    snapshot::{self, Record},
    DriverRunState, DriverStatus, DspMeasurement, LoopSlotId, Peaks, SlotId, StateRecord,
    Telemetry, TimingState,
};
use shoop_rt_transport::rt;

#[derive(Clone, Copy)]
struct Row {
    logical: Identity,
    physical: Identity,
    gain: f32,
    failures: u32,
    last_pulse: Option<u64>,
}

#[derive(Clone, Copy)]
struct FutureAttempt {
    program: SwitchProgram,
    frame: u64,
    program_lane: bool,
}

pub struct Owner<B> {
    session: ProgramSession<B>,
    execution: rt::Execution,
    images: ImageOwner,
    geometry: Geometry,
    driver: DriverStatus,
    status: Arc<Status>,
    ready: Option<SwitchProgram>,
    announced: bool,
    cancelled: bool,
    retry_switch_after: Option<u64>,
    future_attempt: Option<FutureAttempt>,
    retired: Option<ProgramImage>,
    rows: Vec<Row>,
    next_rows: Vec<Row>,
    changed_sources: Vec<Identity>,
    snapshot: Vec<u8>,
    audio_enabled: bool,
    frame: u64,
    published: u64,
    storage_frame: u64,
}

pub fn prepare<B: RegisterBus>(
    mut mailbox: Mailbox<B>,
    driver: DriverStatus,
) -> Result<(Endpoint<Images>, Owner<B>)>
where
    B::Error: Debug,
{
    let geometry = device_result(mailbox.geometry())?;
    let limits = ProgramLimits {
        quantum: 32,
        ..program_limits()
    };
    ensure!(
        geometry.quantum == limits.quantum
            && driver.sample_rate != 0
            && driver.buffer_frames == limits.quantum
            && geometry.chunk_frames == limits.quantum
            && geometry.block_chunks == MEDIA_BLOCK_CHUNKS as u32 / geometry.chunk_frames
            && geometry.instructions == limits.instructions
            && geometry.states == limits.state_values
            && geometry.views >= limits.state_values
            && geometry.jobs != 0
            && geometry.buffers >= limits.buffers
            && geometry.blocks != 0,
        "unsupported looping hardware geometry: {geometry:?}"
    );
    let (control, execution) = prepare_control(limits.quantum, driver, 2 * limits.quantum)?;
    let (images, owner) = image_channel(limits, geometry.chunk_frames);
    let status = Arc::new(Status::new(driver));
    let endpoint = Endpoint::new(control, images, limits.quantum, status.clone());
    let rows = limits.loop_slots as usize;
    Ok((
        endpoint,
        Owner {
            session: ProgramSession::new(Client::new(mailbox)),
            execution,
            images: owner,
            geometry,
            driver,
            status,
            ready: None,
            announced: false,
            cancelled: false,
            retry_switch_after: None,
            future_attempt: None,
            retired: None,
            rows: Vec::with_capacity(rows),
            next_rows: Vec::with_capacity(rows),
            changed_sources: Vec::with_capacity(rows),
            snapshot: vec![0; snapshot::capacity(0, rows, 0).unwrap()],
            audio_enabled: false,
            frame: 0,
            published: 0,
            storage_frame: 0,
        },
    ))
}

fn device_result<T>(result: Result<T, Error<impl Debug>>) -> Result<T> {
    result.map_err(|error| anyhow::anyhow!("FPGA mailbox {error:?}"))
}

fn request<B: RegisterBus>(
    client: &mut Client<B>,
    message: Request,
    within_budget: &mut impl FnMut() -> bool,
) -> Result<[u32; HOST_PAYLOAD_WORDS], Error<B::Error>> {
    let response = client.exchange(message, within_budget)?;
    if response.code != RESULT_OK {
        return Err(Error::Rejected(response.code));
    }
    Ok(response.data)
}

fn join(low: u32, high: u32) -> u64 {
    u64::from(low) | u64::from(high) << 32
}

impl<B: RegisterBus> Owner<B>
where
    B::Error: Debug,
{
    pub fn initialize(&mut self, mut within_budget: impl FnMut() -> bool) -> Result<()> {
        let client = self.session.client();
        let state = device_result(request(
            client,
            Request::capture_snapshot(0),
            &mut within_budget,
        ))?;
        ensure!(state[1..5] == [0, 0, 0, 0], "FPGA requires a fresh reset");
        for view in 0..self.geometry.views {
            device_result(request(
                client,
                Request::store_word(1, view, 1),
                &mut within_budget,
            ))?;
        }
        for (address, value) in [self.geometry.views, 0, 0, self.geometry.blocks]
            .into_iter()
            .enumerate()
        {
            device_result(request(
                client,
                Request::store_word(0, address as u32, value),
                &mut within_budget,
            ))?;
        }
        self.status.publish_storage(StorageUsage {
            audio_blocks: (0, self.geometry.blocks.into()),
            metadata_pages: None,
        });
        self.publish(&mut within_budget)?;
        self.status.update_driver(DriverStatus {
            state: DriverRunState::Running,
            ..self.driver
        });
        Ok(())
    }

    pub fn stop(&mut self, mut within_budget: impl FnMut() -> bool) -> Result<()> {
        device_result(request(
            self.session.client(),
            Request::drain_stop(),
            &mut within_budget,
        ))?;
        self.audio_enabled = false;
        self.status.update_driver(DriverStatus {
            state: DriverRunState::Stopped,
            ..self.driver
        });
        Ok(())
    }

    pub fn fault_record(&mut self) -> Result<Option<rt_fpga_rtl::host::FaultRecord>> {
        device_result(self.session.client().fault_record())
    }

    pub fn fail(&mut self) {
        let _ = self.session.client().abort();
        for commands in
            core::iter::once(&mut self.execution.commands).chain(self.execution.switches.iter_mut())
        {
            commands.begin_callback();
            while let Some(group) = commands.pop() {
                let header = group.header.complete(self.frame, Code::CannotApply);
                group.respond(header, |command| command.complete(Code::CannotApply));
            }
        }
        // A failing guest retains the PL's assigned DDR region until full hardware reset.
        self.status.update_driver(DriverStatus {
            state: DriverRunState::Failed,
            ..self.driver
        });
    }

    /// Service a bounded number of uploads per turn so the control task can run.
    pub fn step(&mut self, mut within_budget: impl FnMut() -> bool) -> Result<()> {
        self.drain_sync()?;
        let switched = self.finish_future_switch(&mut within_budget)?;
        if self.future_attempt.is_some() {
            return Ok(());
        }
        self.upload(&mut within_budget)?;
        let updated = self.command(&mut within_budget)? || switched;
        if self.future_attempt.is_some() {
            return Ok(());
        }
        if self.audio_enabled || updated {
            let state = device_result(request(
                self.session.client(),
                Request::capture_snapshot(0),
                &mut within_budget,
            ))?;
            let frame = join(state[1], state[2]);
            ensure!(frame >= self.frame, "FPGA frame moved backwards");
            self.frame = frame;
            self.status.set_frame(frame);
        }
        if updated
            || self.frame.saturating_sub(self.published) >= u64::from(self.driver.sample_rate / 30)
        {
            let metrics = device_result(self.session.client().metrics())?;
            self.status.observe_fpga(
                metrics.completed_quanta,
                metrics.audio_xruns,
                DspMeasurement::FpgaQuantum {
                    cycles: metrics.peak_quantum_cycles.into(),
                    clock_hz: metrics.clock_hz,
                    frames: self.geometry.quantum,
                    sample_rate: self.driver.sample_rate,
                },
            );
            self.publish(&mut within_budget)?;
        }
        if self.frame.saturating_sub(self.storage_frame) >= self.status.usage_interval() {
            let storage = device_result(request(
                self.session.client(),
                Request::storage_usage(),
                &mut within_budget,
            ))?;
            self.status.publish_storage(StorageUsage {
                audio_blocks: (storage[0].into(), storage[1].into()),
                metadata_pages: Some((storage[2].into(), storage[3].into())),
            });
            self.storage_frame = self.frame;
        }
        Ok(())
    }

    fn finish_future_switch(&mut self, within_budget: &mut impl FnMut() -> bool) -> Result<bool> {
        let Some(attempt) = self.future_attempt else {
            return Ok(false);
        };
        let status = device_result(request(
            self.session.client(),
            Request::capture_snapshot(0),
            within_budget,
        ))?;
        let frame = join(status[1], status[2]);
        if frame == attempt.frame {
            return Ok(false);
        }
        ensure!(
            frame
                .checked_sub(attempt.frame)
                .is_some_and(|elapsed| elapsed >= u64::from(self.geometry.quantum)),
            "FPGA future switch has not run a complete quantum"
        );
        self.frame = frame;
        self.status.set_frame(frame);
        self.future_attempt = None;
        let revision = join(status[3], status[4]);
        if revision
            == self
                .session
                .active()
                .map_or(0, |image| image.layout.revision)
        {
            return Ok(false);
        }
        self.retired = device_result(self.session.accept_future_switch(
            attempt.program,
            revision,
            attempt.frame,
        ))?
        .map(|upload| upload.image);
        self.select_rows();
        self.ready = None;
        self.announced = false;
        self.cancelled = false;
        self.retry_switch_after = None;
        let commands = if attempt.program_lane {
            self.execution
                .switches
                .as_mut()
                .expect("prepared switch lane")
        } else {
            &mut self.execution.commands
        };
        commands.begin_callback();
        let group = commands.pop().expect("armed FPGA switch");
        ensure!(
            group
                .commands()
                .next()
                .is_some_and(|record| record.command() == Command::SwitchProgram(attempt.program)),
            "FPGA future switch command moved"
        );
        let header = group.header;
        group.respond(header.complete(attempt.frame, Code::Applied), |record| {
            record.complete(Code::Applied)
        });
        Ok(true)
    }

    fn drain_sync(&mut self) -> Result<()> {
        let revision = self.session.active().map_or(0, |p| p.layout.revision);
        let future_revision = self.future_attempt.map(|attempt| attempt.program.revision);
        for _ in 0..128 {
            let Some(event) = device_result(self.session.client().sync_event())? else {
                break;
            };
            if event.revision != revision && Some(event.revision) != future_revision {
                continue;
            }
            if let Some(row) = self
                .rows
                .iter_mut()
                .find(|row| row.physical.slot == event.data[1])
            {
                match event.data[0] & 0xff {
                    2 => row.last_pulse = Some(join(event.data[6], event.data[7])),
                    3 if (event.data[0] >> 16) & 0xff == 3 => {
                        row.failures = row.failures.saturating_add(1);
                    }
                    _ => (),
                }
            }
        }
        Ok(())
    }

    fn upload(&mut self, within_budget: &mut impl FnMut() -> bool) -> Result<()> {
        if let Some(image) = self.retired.take() {
            if let Err(rtrb::PushError::Full(image)) = self.images.retired.push(image) {
                self.retired = Some(image);
            }
        }
        if self.retired.is_some() {
            return Ok(());
        }
        if let Ok(request) = self.images.uploads.pop() {
            match request {
                super::UploadRequest::Stage(upload) => {
                    if let Err(upload) = self.session.stage(upload) {
                        let revision = upload.image.layout.revision;
                        self.retired = Some(upload.image);
                        ensure!(
                            self.images.ready.push((revision, false)).is_ok(),
                            "FPGA upload result queue full"
                        );
                        return Ok(());
                    }
                }
                super::UploadRequest::Cancel(revision) => {
                    if let Some(image) = self
                        .session
                        .staged()
                        .filter(|image| image.layout.revision == revision)
                    {
                        self.ready = Some(descriptor(image));
                        self.cancelled = true;
                    }
                }
            }
        }
        if self.session.staged().is_none() {
            return Ok(());
        }
        if !self.announced {
            for _ in 0..24 {
                if self.ready.is_some() {
                    break;
                }
                self.ready = device_result(self.session.upload_step(&mut *within_budget))?;
            }
            if let Some(ready) = self.ready {
                self.announced = self.images.ready.push((ready.revision, true)).is_ok();
            }
        }
        Ok(())
    }

    fn command(&mut self, within_budget: &mut impl FnMut() -> bool) -> Result<bool> {
        let ordinary = self.command_lane(false, within_budget)?;
        let switched = self.future_attempt.is_none() && self.command_lane(true, within_budget)?;
        Ok(ordinary || switched)
    }

    fn command_lane(
        &mut self,
        program_lane: bool,
        within_budget: &mut impl FnMut() -> bool,
    ) -> Result<bool> {
        let commands = if program_lane {
            self.execution
                .switches
                .as_mut()
                .expect("prepared switch lane")
        } else {
            &mut self.execution.commands
        };
        commands.begin_callback();
        let Some(group) = commands.pop() else {
            return Ok(false);
        };
        let header = group.header;
        let mut records = [CommandRecord::new(Command::LoopGain(0, 0.)); COMMANDS];
        let mut count = 0;
        for record in group.commands() {
            records[count] = *record;
            count += 1;
        }
        group.leave_unread();
        let records = &mut records[..count];
        if program_lane
            && !self.cancelled
            && self
                .retry_switch_after
                .is_some_and(|frame| self.frame <= frame)
        {
            return Ok(false);
        }
        if self.retired.is_some()
            && records
                .iter()
                .any(|r| matches!(r.command(), Command::SwitchProgram(_)))
        {
            return Ok(false);
        }
        let revision = self.session.active().map_or(0, |p| p.layout.revision);
        let outcome = if (header.graph(), header.topology()) != (revision, revision) {
            Ok(Some((self.frame, Code::StaleConfiguration)))
        } else if records
            .iter()
            .enumerate()
            .any(|(i, r)| matches!(r.command(), Command::SwitchProgram(_)) && i + 1 != count)
        {
            Ok(Some((self.frame, Code::CannotApply)))
        } else {
            self.apply(
                revision,
                header.deadline(),
                records,
                program_lane,
                within_budget,
            )
        };
        if matches!(outcome, Ok(None)) {
            return Ok(false);
        }
        if program_lane {
            self.retry_switch_after = None;
        }
        let outcome = outcome.map(|result| result.expect("completed control attempt"));
        if outcome
            .as_ref()
            .is_ok_and(|(_, code)| *code != Code::Applied)
        {
            for record in records.iter() {
                if let Command::SwitchProgram(requested) = record.command() {
                    if let Some(upload) = self.session.discard_staged(requested) {
                        self.retired = Some(upload.image);
                        self.ready = None;
                        self.announced = false;
                        self.cancelled = false;
                    }
                }
            }
        }
        let commands = if program_lane {
            self.execution
                .switches
                .as_mut()
                .expect("prepared switch lane")
        } else {
            &mut self.execution.commands
        };
        commands.begin_callback();
        let group = commands.pop().expect("queued FPGA command group");
        let (frame, code) = outcome
            .as_ref()
            .copied()
            .unwrap_or((self.frame, Code::CannotApply));
        let mut responses = records.iter();
        group.respond(header.complete(frame, code), |record| {
            let response = *responses.next().expect("FPGA command response");
            if code == Code::Applied {
                response
            } else {
                record.complete(code)
            }
        });
        outcome.map(|(_, code)| code == Code::Applied)
    }

    fn apply(
        &mut self,
        revision: u64,
        deadline: Option<u64>,
        records: &mut [CommandRecord],
        program_lane: bool,
        within_budget: &mut impl FnMut() -> bool,
    ) -> Result<Option<(u64, Code)>> {
        let switching = records.last().and_then(|record| match record.command() {
            Command::SwitchProgram(candidate) => Some(candidate),
            _ => None,
        });
        self.changed_sources.clear();
        if let Some(next) = switching {
            if !self.announced || self.ready != Some(next) || self.retired.is_some() {
                return Ok(Some((self.frame, Code::CannotApply)));
            }
            if self.cancelled {
                return Ok(Some((self.frame, Code::Cancelled)));
            }
            let staged = self.session.staged().filter(|p| descriptor(p) == next);
            let staged = staged.ok_or_else(|| anyhow::anyhow!("FPGA staged image mismatch"))?;
            self.changed_sources
                .extend_from_slice(staged.layout.changed_sources());
        }

        let composite_wrap_frame = self.session.composite_wrap_frame();
        let (active, staged, client) = self.session.programs_and_client();
        let mut window =
            device_result(ControlWindow::begin(client, revision, &mut *within_budget))?;
        let frame = window.frame();
        if deadline.is_some_and(|deadline| frame > deadline) {
            device_result(window.release())?;
            return Ok(Some((frame, Code::DeadlineMissed)));
        }
        if switching.is_some() {
            if composite_wrap_frame == Some(frame) {
                device_result(window.release())?;
                self.retry_switch_after = Some(frame);
                return Ok(None);
            }
            let layout = &staged.unwrap().layout;
            if let Some((program, cut)) = switching
                .filter(|_| records.len() == 1)
                .zip(layout.future_switch())
            {
                device_result(window.arm_future_switch(program, cut))?;
                device_result(window.release())?;
                self.future_attempt = Some(FutureAttempt {
                    program,
                    frame,
                    program_lane,
                });
                return Ok(None);
            }
            let cuts = layout.composite_cuts();
            let mut status = Code::Applied;
            if let Some(slot) = layout.retiring_composite() {
                status = device_result(window.prepare_composite_retirement(slot))?;
            } else {
                for cut in cuts {
                    status =
                        device_result(window.prepare_composite_cut(cut, self.geometry.quantum))?;
                    if status != Code::Applied {
                        break;
                    }
                }
            }
            if status != Code::Applied {
                device_result(window.release())?;
                if status == Code::Scheduled && cuts.len() == 1 && records.len() == 1 {
                    self.retry_switch_after = Some(frame);
                    return Ok(None);
                }
                return Ok(Some((frame, Code::CannotApply)));
            }
        }
        for record in records {
            let command = record.command();
            let code = if !command.valid() {
                Code::InvalidValue
            } else if matches!(command, Command::SwitchProgram(_)) {
                for logical in &self.changed_sources {
                    let row = self
                        .rows
                        .iter_mut()
                        .find(|row| row.logical == *logical)
                        .ok_or_else(|| anyhow::anyhow!("missing continuing FPGA loop"))?;
                    let result =
                        device_result(window.clear_transitions(row.physical, ClearPlanned::Both))?;
                    ensure!(
                        result == Code::Applied,
                        "could not clear the continuing FPGA loop plan"
                    );
                    row.last_pulse = None;
                }
                Code::Applied
            } else {
                let target = match command {
                    Command::Transition(id, _)
                    | Command::ClearPlannedTransitions(id, _)
                    | Command::LoopGain(id, _) => id,
                    Command::LoopIteration(id, _) | Command::LoopRepeatSync(id, _) => id.0.index,
                    _ => {
                        *record = record.complete(Code::Unsupported);
                        continue;
                    }
                };
                match self.rows.iter_mut().find(|row| row.logical.slot == target) {
                    Some(row) => {
                        device_result(edit_loop(&mut window, active.unwrap(), row, command))?
                    }
                    None => Code::TargetUnavailable,
                }
            };
            *record = record.complete(code);
        }
        let fence = switching
            .map(|next| device_result(window.switch_program(next)))
            .transpose()?;
        device_result(window.release())?;
        if let Some(fence) = fence {
            let (selected, retired) = device_result(self.session.accept_fence(fence))?;
            self.retired = retired.map(|upload| upload.image);
            self.select_rows();
            self.ready = None;
            self.announced = false;
            if !self.audio_enabled {
                device_result(request(
                    self.session.client(),
                    Request::enable_audio(true),
                    within_budget,
                ))?;
                self.audio_enabled = true;
            }
            return Ok(Some((selected.frame, Code::Applied)));
        }
        Ok(Some((frame, Code::Applied)))
    }

    fn select_rows(&mut self) {
        let image = self.session.active().unwrap();
        for allocation in &image.layout.loops {
            let continuing = self.rows.iter().find(|row| {
                row.logical == allocation.logical && row.physical == allocation.physical
            });
            let row = continuing.copied().unwrap_or_else(|| {
                let gain = image
                    .compiled
                    .bindings
                    .iter()
                    .find(|binding| binding.id == allocation.physical)
                    .and_then(|binding| binding.gain)
                    .map_or(1., |address| {
                        Fixed(image.compiled.state[address as usize].get(32, 32) as i32).to_f32()
                    });
                Row {
                    logical: allocation.logical,
                    physical: allocation.physical,
                    gain,
                    failures: 0,
                    last_pulse: None,
                }
            });
            self.next_rows.push(row);
        }
        core::mem::swap(&mut self.rows, &mut self.next_rows);
        self.next_rows.clear();
    }

    fn publish(&mut self, within_budget: &mut impl FnMut() -> bool) -> Result<()> {
        let extent = self
            .rows
            .iter()
            .map(|row| row.physical.slot + 1)
            .max()
            .unwrap_or(0);
        let (active, staged, client) = self.session.programs_and_client();
        let latch = device_result(request(
            client,
            Request::capture_snapshot(extent),
            within_budget,
        ))?;
        let frame = join(latch[1], latch[2]);
        let revision = join(latch[3], latch[4]);
        let mut offset = snapshot::HEADER;
        let mut records = 0;
        let staged = staged
            .filter(|_| !self.cancelled)
            .map(|image| (&image.layout, self.announced));
        for row in &self.rows {
            let mut words = [0; 16];
            for (index, block) in words.chunks_mut(HOST_PAYLOAD_WORDS / 2).enumerate() {
                let address = row.physical.slot * 16 + (index * (HOST_PAYLOAD_WORDS / 2)) as u32;
                let data = device_result(request(
                    client,
                    Request::read_snapshot(latch[0], address, block.len() as u32),
                    within_budget,
                ))?;
                for (word, parts) in block.iter_mut().zip(data.chunks_exact(2)) {
                    *word = join(parts[0], parts[1]);
                }
            }
            for record in loop_records(*row, &words) {
                offset += codec::encode_snapshot(&record, &mut self.snapshot[offset..])
                    .map_err(|error| anyhow::anyhow!("FPGA snapshot encoding {error:?}"))?;
                records += 1;
            }
            if let Some(composite) = active.and_then(|image| {
                composite_telemetry(row.logical, frame, words[2], &words, &image.layout, staged)
            }) {
                offset += codec::encode_snapshot(
                    &Record::Telemetry(composite),
                    &mut self.snapshot[offset..],
                )
                .map_err(|error| anyhow::anyhow!("FPGA snapshot encoding {error:?}"))?;
                records += 1;
            }
        }
        snapshot::Header {
            frame,
            graph: revision,
            topology: revision,
            records,
            loops: self.rows.len() as u32,
            strips: 0,
            buses: 0,
        }
        .write(&mut self.snapshot);
        let _ = self.execution.snapshots.publish(&self.snapshot[..offset]);
        self.published = frame;
        Ok(())
    }
}

fn edit_loop<B: RegisterBus, F: FnMut() -> bool>(
    window: &mut ControlWindow<'_, B, F>,
    image: &ProgramImage,
    row: &mut Row,
    command: Command,
) -> Result<Code, Error<B::Error>> {
    match command {
        Command::LoopIteration(id, request) if id.0.generation == row.logical.generation => {
            window.apply_iteration(row.physical, request)
        }
        Command::LoopRepeatSync(id, enabled) if id.0.generation == row.logical.generation => {
            window.set_repeat_sync(row.physical, enabled)
        }
        Command::LoopIteration(..) | Command::LoopRepeatSync(..) => Ok(Code::TargetUnavailable),
        Command::Transition(_, plan) => {
            let source = image
                .layout
                .loops
                .iter()
                .find(|allocation| allocation.physical == row.physical)
                .and_then(|allocation| allocation.sync_source)
                .map(|logical| image.layout.loop_id(logical))
                .transpose()
                .map_err(|_| Error::InvalidRequest)?
                .map(|id| LoopId {
                    slot: id.slot,
                    generation: id.generation,
                });
            window.apply_transition(row.physical, source, row.last_pulse, plan)
        }
        Command::ClearPlannedTransitions(_, mask) => window.clear_transitions(row.physical, mask),
        Command::LoopGain(_, gain) => {
            let address = image
                .compiled
                .bindings
                .iter()
                .find(|binding| binding.id == row.physical)
                .and_then(|binding| binding.gain);
            let Some(address) = address else {
                return Ok(Code::Unsupported);
            };
            window.set_gain(u32::from(address), gain)?;
            row.gain = gain;
            Ok(Code::Applied)
        }
        _ => Ok(Code::Unsupported),
    }
}

fn loop_records(row: Row, words: &[u64; 16]) -> [Record; 4] {
    let state = loop_state(row.physical, words, row.last_pulse);
    let plan = loop_plan(words);
    let id = LoopSlotId(SlotId::new(row.logical.slot, row.logical.generation));
    let next = [plan.first, plan.second]
        .into_iter()
        .flatten()
        .min_by_key(|step| step.skip);
    let timing = TimingState {
        content_revision: state.content_revision,
        length: state.content_length,
        raw_audio_frames: state.content_length,
        ..Default::default()
    };
    [
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
            next_mode: next.map(|step| protocol_mode(reference_mode(step.mode))),
            next_delay: next.map(|step| step.skip),
            content_revision: state.content_revision,
            recording_armed: [plan.first, plan.second]
                .into_iter()
                .flatten()
                .any(|step| step.mode == TransitionMode::Recording),
            capture_incomplete: false,
            timing_revision: timing.revision,
            length: if state.mode == Mode::Recording {
                state.capture_frames
            } else {
                state.content_length
            },
            position: state.position,
            peaks: Peaks::default(),
        }),
    ]
}
