use super::*;
use crate::{storage::ViewId, target::ProgramImage};
use rt_fpga_compiler::ProgramLimits;
use shoop_rt_protocol::immediate::{CommandRecord, Header, SwitchProgram, COMMANDS};

pub struct PreparedProgram {
    pub image: ProgramImage,
    descriptor: SwitchProgram,
    rows: Vec<Loop>,
    views: Vec<ViewId>,
    prepared: usize,
    error: Option<Error>,
    ready_sent: bool,
    pub(super) cancelled: bool,
}
impl PreparedProgram {
    pub fn new(image: ProgramImage, unused_views: Vec<ViewId>) -> Result<Box<Self>, Error> {
        let count = |n: usize| u16::try_from(n).map_err(|_| Error::InvalidShape);
        let c = &image.compiled;
        let descriptor = SwitchProgram {
            revision: image.layout.revision,
            region: u16::from(image.layout.region),
            instructions: count(c.program.instructions.len() + c.program.sync_instructions.len())?,
            sync_count: count(c.program.sync_instructions.len())?,
            states: count(c.memory.len())?,
            jobs: count(c.program.jobs_per_quantum)?,
            buffers: count(c.resources.buffers as usize)?,
        };
        let mut rows = Vec::new();
        for b in &c.bindings {
            let logical = image
                .layout
                .loops
                .iter()
                .find(|l| (l.physical.slot, l.physical.generation) == (b.id.slot, b.id.generation))
                .ok_or(Error::InvalidState)?
                .logical;
            let gain = b
                .gain
                .map(|a| c.memory[a.0].ramp().map(|r| r.target.to_f32()))
                .transpose()?
                .unwrap_or(1.0);
            rows.push(Loop {
                binding: LoopBinding {
                    id: b.id,
                    public_id: LoopId {
                        slot: logical.slot,
                        generation: logical.generation,
                    },
                    gain: b.gain,
                    meter: b.meter,
                    initial_gain: gain,
                    timing: TimingState::default(),
                },
                gain,
                peak: 0.0,
            });
        }
        rows.sort_unstable_by_key(|l| l.binding.public_id);
        Ok(Box::new(Self {
            image,
            descriptor,
            rows,
            views: unused_views,
            prepared: 0,
            error: None,
            ready_sent: false,
            cancelled: false,
        }))
    }
    pub fn descriptor(&self) -> SwitchProgram {
        self.descriptor
    }
    pub(super) fn ready(&self) -> bool {
        self.ready_sent && self.error.is_none() && !self.cancelled
    }
}
#[derive(Clone, Copy, Debug)]
pub struct ProgramReady {
    pub revision: u64,
    pub result: Result<(), Error>,
}
enum Request {
    Stage(Box<PreparedProgram>),
    Discard(u64),
    Cancel(u64),
}
pub struct ProgramControl {
    upload: rtrb::Producer<Request>,
    ready: rtrb::Consumer<ProgramReady>,
    retired: rtrb::Consumer<Box<PreparedProgram>>,
}
pub struct ProgramExecution {
    upload: rtrb::Consumer<Request>,
    ready: rtrb::Producer<ProgramReady>,
    retired: rtrb::Producer<Box<PreparedProgram>>,
}
pub fn program_memory() -> (ProgramControl, ProgramExecution) {
    let (tx, rx) = rtrb::RingBuffer::new(2);
    let (ack, ready) = rtrb::RingBuffer::new(2);
    let (retire, retired) = rtrb::RingBuffer::new(2);
    (
        ProgramControl {
            upload: tx,
            ready,
            retired,
        },
        ProgramExecution {
            upload: rx,
            ready: ack,
            retired: retire,
        },
    )
}
impl ProgramControl {
    pub fn stage(&mut self, image: Box<PreparedProgram>) -> Result<(), Box<PreparedProgram>> {
        self.upload
            .push(Request::Stage(image))
            .map_err(|r| match r {
                rtrb::PushError::Full(Request::Stage(image)) => image,
                _ => unreachable!(),
            })
    }
    pub fn discard(&mut self, revision: u64) -> bool {
        self.upload.push(Request::Discard(revision)).is_ok()
    }
    pub fn cancel(&mut self, revision: u64) -> bool {
        self.upload.push(Request::Cancel(revision)).is_ok()
    }
    pub fn ready(&mut self) -> Option<ProgramReady> {
        self.ready.pop().ok()
    }
    pub fn reclaim(&mut self) -> Option<Box<PreparedProgram>> {
        self.retired.pop().ok()
    }
}
pub(super) struct PendingSwitch {
    pub program_lane: bool,
    pub header: Header,
    pub frame: u64,
    pub records: [CommandRecord; COMMANDS],
    pub count: usize,
    pub retired: Box<PreparedProgram>,
}
impl Processor {
    pub fn with_program_memory(
        mut self,
        ports: ProgramExecution,
        limits: ProgramLimits,
    ) -> Result<Self, Error> {
        self.engine
            .pipeline
            .reserve_loading(limits)
            .map_err(|_| Error::InvalidShape)?;
        let bytes =
            snapshot::capacity(0, limits.loop_slots as usize, 0).ok_or(Error::InvalidShape)?;
        if bytes > self.ports.snapshots.capacity() {
            return Err(Error::InvalidShape);
        }
        self.snapshot_bytes = vec![0; bytes].into_boxed_slice();
        self.programs = Some(ports);
        Ok(self)
    }
    pub(super) fn prepare_staging(
        &mut self,
        memory: &mut impl Memory,
    ) -> Result<(), PipelineError> {
        let Some(ports) = &mut self.programs else {
            return Ok(());
        };
        if let Some(image) = self.retired_program.take() {
            if let Err(rtrb::PushError::Full(image)) = ports.retired.push(image) {
                self.retired_program = Some(image);
                return Ok(());
            }
        }
        if let Ok(request) = ports.upload.pop() {
            match request {
                Request::Stage(image) => {
                    assert!(self.next_program.is_none(), "one staged program");
                    self.next_program = Some(image);
                }
                Request::Discard(revision) => {
                    if self
                        .next_program
                        .as_ref()
                        .is_some_and(|p| p.descriptor.revision == revision)
                    {
                        assert!(
                            self.retired_program.is_none(),
                            "bounded reclamation storage"
                        );
                        self.retired_program = self.next_program.take();
                    }
                }
                Request::Cancel(revision) => {
                    if let Some(image) = self
                        .next_program
                        .as_mut()
                        .filter(|image| image.descriptor.revision == revision)
                    {
                        image.cancelled = true;
                    }
                }
            }
        }
        if let Some(image) = &mut self.next_program {
            if !image.ready_sent {
                if !image.cancelled && image.prepared < image.views.len() && image.error.is_none() {
                    match memory.prepare_unused_view(image.views[image.prepared]) {
                        Ok(true) => image.prepared += 1,
                        Ok(false) => {}
                        Err(error) => image.error = Some(error),
                    }
                }
                if image.cancelled || image.error.is_some() || image.prepared == image.views.len() {
                    let ready = ProgramReady {
                        revision: image.descriptor.revision,
                        result: image.error.map_or(Ok(()), Err),
                    };
                    image.ready_sent = ports.ready.push(ready).is_ok();
                    if image.ready_sent && image.error.is_some() {
                        assert!(
                            self.retired_program.is_none(),
                            "bounded reclamation storage"
                        );
                        self.retired_program = self.next_program.take();
                    }
                }
            }
        }
        Ok(())
    }
}
impl Engine {
    pub(super) fn waits_for_composite_cut(&mut self, image: &PreparedProgram) -> bool {
        if self.pipeline.state().composite_wrap_pending() {
            return true;
        }
        if let Some(slot) = image.image.layout.retiring_composite() {
            let id = LoopId {
                slot: slot.slot,
                generation: slot.generation,
            };
            if self.pipeline.state().loop_state(id).is_ok_and(|slot| {
                slot.state.mode == Mode::Playing || slot.planned.iter().any(Option::is_some)
            }) {
                return self
                    .pipeline
                    .write_mode_plan(
                        id,
                        ModePlan {
                            now: Some(Mode::Stopped),
                            ..ModePlan::default()
                        },
                    )
                    .is_ok();
            }
        }
        let [cut] = image.image.layout.composite_cuts() else {
            return false;
        };
        let state = self.pipeline.state();
        state
            .loop_state(LoopId {
                slot: cut.old_slot.slot,
                generation: cut.old_slot.generation,
            })
            .is_ok_and(|slot| {
                if slot.planned.iter().any(Option::is_some)
                    && !cut.preserves_transport_at_finish(slot.state.at_end())
                {
                    return cut.waits_for_control();
                }
                slot.state.mode == Mode::Playing
                    && cut.waits_for_boundary(
                        slot.state.position,
                        slot.state.content_length,
                        state.composite_restart_ready(cut, slot),
                        slot.composite_wait,
                    )
            })
    }

    pub(super) fn switch(
        &mut self,
        image: &mut PreparedProgram,
        descriptor: SwitchProgram,
        jobs: &mut JobProcessor,
    ) -> Code {
        if descriptor != image.descriptor || !image.ready() {
            return Code::CannotApply;
        }
        if image.image.layout.expected != self.configuration.graph
            || self.configuration.graph != self.configuration.topology
        {
            return Code::StaleConfiguration;
        }
        if self.pipeline.switch_image(&mut image.image).is_err() {
            return Code::CannotApply;
        }
        for &logical in image.image.layout.changed_sources() {
            let physical = image
                .image
                .layout
                .loop_id(logical)
                .expect("continuing loop");
            self.pipeline
                .clear_mode_plan(
                    LoopId {
                        slot: physical.slot,
                        generation: physical.generation,
                    },
                    shoop_rt_protocol::immediate::ClearPlanned::Both as u8,
                )
                .expect("continuing loop");
        }
        for &history in image.image.layout.new_histories() {
            jobs.initialize_history(history as usize)
                .expect("allocated history");
        }
        for row in &mut image.rows {
            let binding = row.binding;
            if let Some(old) = self
                .loops
                .iter_mut()
                .find(|old| old.binding.id == binding.id)
            {
                core::mem::swap(row, old);
                row.binding = binding;
            }
        }
        core::mem::swap(&mut self.loops, &mut image.rows);
        self.configuration.graph = descriptor.revision;
        self.configuration.topology = descriptor.revision;
        Code::Applied
    }
}
