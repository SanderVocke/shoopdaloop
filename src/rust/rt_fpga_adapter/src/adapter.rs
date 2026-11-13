//! ARM-side owning queue consumer. Only the device processes audio samples.
use crate::stopwatch::Stopwatch;
use crate::{
    compiler::{self, NodeDescriptor},
    image::*,
};
use alloc::{vec, vec::Vec};
use core::time::Duration;
use rt_runtime::{
    Epoch, Error, Frame, PreparedConfig, Q824, Quantum,
    executor::{Command, Control, Diagnostics, Ingress, PreparedNode, Retired},
    graph::{Catalog, NodeSpec, PlanValidator, PreparedPlan, ValidationStatus},
    planner::NoPlanner,
    schedule::{Action, Scheduler},
    transport::{Lane, Outcome},
};
use shoop_rt_engine::dsp::{GainOperation, GainProcessor};

pub type Processor = GainProcessor<Q824>;
pub type Operation = GainOperation<Q824>;
pub type Controller = Control<Processor, NoPlanner, Operation, Q824>;
pub type Request = Command<Processor, NoPlanner, Operation, Q824>;
pub type Node = PreparedNode<Processor, NoPlanner, Operation, Q824>;

pub trait Device {
    fn load(&mut self, image: &Image) -> Result<(), Error>;
    fn write_state(&mut self, address: u16, value: Value) -> Result<(), Error>;
    fn process(&mut self, image: &Image, inputs: &[Vec<i32>]) -> Result<Vec<Vec<i32>>, Error>;
}
#[derive(Clone, Debug, Default)]
pub struct Metrics {
    pub compilations: u64,
    pub compile_time: Duration,
    pub configuration_time: Duration,
    pub instructions: usize,
    pub state_words: usize,
    pub image_bytes: usize,
    pub gain_writes: u64,
}
struct Slot {
    spec: NodeSpec,
    processor: Option<Processor>,
}
pub struct Adapter<D: Device> {
    device: D,
    config: PreparedConfig,
    ingress: Ingress<Processor, NoPlanner, Operation, Q824>,
    scheduler: Scheduler<(), (), Operation, ()>,
    slots: Vec<Slot>,
    active: Option<PreparedPlan>,
    image: Option<Image>,
    frame: Frame,
    epoch: Epoch,
    fault: Option<Error>,
    pub metrics: Metrics,
}
impl<D: Device> Adapter<D> {
    pub fn prepare(
        config: PreparedConfig,
        nodes: Vec<Node>,
        device: D,
    ) -> Result<(Controller, Self), Error> {
        let limits = config.limits();
        if limits.audio_inputs != 1
            || limits.audio_outputs != 1
            || limits.midi_inputs != 0
            || limits.midi_outputs != 0
            || nodes.len() > 2
            || limits.max_frames < 32
        {
            return Err(Error::InvalidConfig);
        }
        Catalog::new(config, &nodes.iter().map(Node::spec).collect::<Vec<_>>())?;
        for node in &nodes {
            Self::supported(node)?;
        }
        let (control, ingress, scheduler) =
            Controller::channel(config, Quantum::new(32)?, 0, false)?;
        let slots = nodes
            .into_iter()
            .map(|node| Slot {
                spec: node.spec(),
                processor: Some(node.into_processor()),
            })
            .collect();
        Ok((
            control,
            Self {
                device,
                config,
                ingress,
                scheduler,
                slots,
                active: None,
                image: None,
                frame: Frame(0),
                epoch: Epoch(0),
                fault: None,
                metrics: Metrics::default(),
            },
        ))
    }
    fn supported(node: &Node) -> Result<(), Error> {
        if node.processor().channels() != 1
            || node.processor().frames() != 32
            || !node.processor().steady()
        {
            return Err(Error::InvalidParameter);
        }
        Ok(())
    }
    pub fn frame(&self) -> Frame {
        self.frame
    }
    pub fn epoch(&self) -> Epoch {
        self.epoch
    }
    pub fn fault(&self) -> Option<Error> {
        self.fault
    }
    pub fn device(&self) -> &D {
        &self.device
    }
    pub fn device_mut(&mut self) -> &mut D {
        &mut self.device
    }
    pub fn image(&self) -> Option<&Image> {
        self.image.as_ref()
    }
    fn compile(
        config: PreparedConfig,
        slots: &[Slot],
        plan: &PreparedPlan,
    ) -> Result<Image, Error> {
        let specs = slots
            .iter()
            .filter(|s| s.processor.is_some())
            .map(|s| s.spec)
            .collect::<Vec<_>>();
        let catalog = Catalog::new(config, &specs)?;
        let mut validator = PlanValidator::new(config);
        validator.begin();
        loop {
            match validator.advance(plan, &catalog, 1024).status {
                ValidationStatus::Valid => break,
                ValidationStatus::Invalid(e) => return Err(e),
                ValidationStatus::Pending => {}
            }
        }
        let descriptors = slots
            .iter()
            .filter_map(|s| {
                s.processor.as_ref().map(|p| NodeDescriptor {
                    id: Identity {
                        slot: s.spec.id.slot(),
                        generation: s.spec.id.generation(),
                    },
                    channels: 1,
                    operation: NodeKind::Gain {
                        channels: 1,
                        initial: p.gain().0,
                    },
                })
            })
            .collect::<Vec<_>>();
        if plan
            .descriptor()
            .connections
            .iter()
            .any(|e| e.source.kind != rt_runtime::graph::PortType::Audio)
        {
            return Err(Error::InvalidParameter);
        }
        let image = compiler::lower(
            plan.descriptor(),
            &descriptors,
            Image {
                format_version: 1,
                name: "runtime-gain-graph".into(),
                transition: TransitionImage {
                    description: Description {
                        quantum: 32,
                        loops: vec![],
                    },
                    limits: Limits {
                        loop_slots: 1,
                        pending_requests: 2,
                        events: 4,
                    },
                    dependency_order: vec![],
                },
                program: Program {
                    instructions: vec![],
                    quantum: 32,
                    chunk_frames: 32,
                    jobs_per_quantum: 0,
                },
                initial_state: Memory { memory: vec![] },
                buffers: 0,
                input_ports: 1,
                output_ports: 1,
                media: MediaImage {
                    chunk_frames: 32,
                    samples: vec![],
                    free_chunks: vec![],
                    views: vec![],
                },
                loop_views: vec![],
                gains: vec![],
                node_order: vec![],
            },
        )
        .map_err(|_| Error::InvalidGraph)?;
        if image.program.instructions.len() > 64
            || image.initial_state.memory.len() > 32
            || image.buffers > 16
            || image.program.jobs_per_quantum > 64
        {
            return Err(Error::Capacity);
        }
        Ok(image)
    }
    pub fn service(&mut self) -> Result<(), Error> {
        self.scheduler.begin_callback();
        self.ingress.begin_cycle(self.frame, self.epoch);
        while let Some(operation) = self.ingress.next_operation() {
            let mut retired = None;
            let mut admission = None;
            let result = (|| -> Result<(), Error> {
                if let Some(error) = self.fault {
                    return Err(error);
                }
                match operation.request {
                    Request::Install { prepared } => {
                        if operation.lane != Lane::Normal {
                            return Err(Error::InvalidState);
                        }
                        if self.active.is_some() {
                            return Err(Error::Busy);
                        }
                        let node = prepared.as_ref().ok_or(Error::InvalidState)?;
                        Self::supported(node)?;
                        let slot = self
                            .slots
                            .iter_mut()
                            .find(|s| s.spec.id == node.spec().id)
                            .ok_or(Error::InvalidId)?;
                        if slot.processor.is_some() {
                            return Err(Error::InvalidId);
                        }
                        if slot.spec.ports != node.spec().ports {
                            return Err(Error::InvalidShape);
                        }
                        let epoch = self.epoch.checked_next()?;
                        slot.processor =
                            Some(prepared.take().ok_or(Error::InvalidState)?.into_processor());
                        self.epoch = epoch;
                        Ok(())
                    }
                    Request::Remove { node } => {
                        if operation.lane != Lane::Normal {
                            return Err(Error::InvalidState);
                        }
                        if self.active.is_some() {
                            return Err(Error::Busy);
                        }
                        let slot = self
                            .slots
                            .iter_mut()
                            .find(|s| s.spec.id == *node && s.processor.is_some())
                            .ok_or(Error::InvalidId)?;
                        let next = node.checked_next_generation()?;
                        let epoch = self.epoch.checked_next()?;
                        retired = slot.processor.take().map(Retired::Processor);
                        slot.spec.id = next;
                        self.epoch = epoch;
                        Ok(())
                    }
                    Request::Commit {
                        expected_epoch,
                        plan,
                    } => {
                        if operation.lane != Lane::Normal {
                            return Err(Error::InvalidState);
                        }
                        if *expected_epoch != self.epoch {
                            return Err(Error::StaleEpoch);
                        }
                        if self.active.is_some() || self.frame != Frame(0) {
                            return Err(Error::Busy);
                        }
                        let next = self.epoch.checked_next()?;
                        let start = Stopwatch::start();
                        let image = Self::compile(
                            self.config,
                            &self.slots,
                            plan.as_ref().ok_or(Error::InvalidGraph)?,
                        )?;
                        self.metrics.compile_time += start.elapsed();
                        self.metrics.compilations += 1;
                        let start = Stopwatch::start();
                        if let Err(e) = self.device.load(&image) {
                            self.fault = Some(e);
                            return Err(e);
                        }
                        self.metrics.configuration_time += start.elapsed();
                        self.metrics.instructions = image.program.instructions.len();
                        self.metrics.state_words = image.initial_state.memory.len();
                        self.metrics.image_bytes = self.metrics.instructions * 12
                            + self.metrics.state_words * 17
                            + 80
                            + 80
                            + 4 * 20;
                        self.image = Some(image);
                        self.active = plan.take();
                        self.epoch = next;
                        Ok(())
                    }
                    Request::Schedule {
                        at,
                        action,
                        payload,
                    } => {
                        if operation.lane != Lane::Normal {
                            return Err(Error::InvalidState);
                        }
                        if !payload.is_empty() {
                            return Err(Error::InvalidParameter);
                        }
                        let Action::Processor {
                            node,
                            operation: gain,
                        } = *action
                        else {
                            return Err(Error::InvalidParameter);
                        };
                        if !self
                            .slots
                            .iter()
                            .any(|s| s.spec.id == node && s.processor.is_some())
                        {
                            return Err(Error::InvalidId);
                        }
                        if self.image.as_ref().is_none_or(|i| {
                            !i.gains.iter().any(|b| {
                                b.id.slot == node.slot() && b.id.generation == node.generation()
                            })
                        }) {
                            return Err(Error::InvalidState);
                        }
                        if gain.duration != 0 {
                            return Err(Error::InvalidParameter);
                        }
                        if at.0 % 32 != 0 {
                            return Err(Error::InvalidFrame);
                        }
                        admission = Some(self.scheduler.admit(
                            operation.sequence,
                            *at,
                            self.frame,
                            *action,
                            payload,
                        )?);
                        Ok(())
                    }
                    Request::Cancel { sequence } => self.scheduler.cancel(self.frame, *sequence),
                    _ => Err(Error::InvalidParameter),
                }
            })();
            self.ingress.set_epoch(self.epoch);
            let outcome = match result {
                Ok(()) => admission.map_or(Outcome::Applied, |a| Outcome::Scheduled {
                    effective_frame: a.effective_frame,
                    late_by: a.late_by,
                }),
                Err(e) => Outcome::Rejected(e),
            };
            self.ingress
                .complete(outcome, retired)
                .map_err(|e| e.error)?;
        }
        if self.fault.is_some() {
            self.cancel_pending_after_fault()?;
        }
        Ok(())
    }
    fn cancel_pending_after_fault(&mut self) -> Result<(), Error> {
        while let Some(event) = self.scheduler.due(Frame(u64::MAX)) {
            let sequence = event.sequence;
            self.scheduler.cancel(self.frame, sequence)?;
        }
        Ok(())
    }
    pub fn process(&mut self, inputs: &[Vec<i32>]) -> Result<Vec<Vec<i32>>, Error> {
        if inputs.len() != 1 || inputs[0].len() != 32 {
            return Err(Error::InvalidShape);
        }
        self.service()?;
        let next = self.frame.checked_add(32)?;
        while let Some(mut event) = self.scheduler.deliver(self.frame) {
            let result = (|| {
                if let Some(error) = self.fault {
                    return Err(error);
                }
                let Action::Processor { node, operation } = event.action else {
                    return Err(Error::InvalidParameter);
                };
                let binding = self
                    .image
                    .as_ref()
                    .ok_or(Error::InvalidState)?
                    .gains
                    .iter()
                    .find(|b| b.id.slot == node.slot() && b.id.generation == node.generation())
                    .ok_or(Error::InvalidId)?;
                for &address in &binding.ramps {
                    if let Err(e) = self.device.write_state(
                        address,
                        Value::Ramp(Ramp {
                            start: operation.target.0,
                            target: operation.target.0,
                            duration: 0,
                            elapsed: 0,
                        }),
                    ) {
                        self.fault = Some(e);
                        return Err(e);
                    }
                    self.metrics.gain_writes += 1;
                }
                Ok(())
            })();
            event.receipts.complete_processor(event.token, result)?;
        }
        if let Some(error) = self.fault {
            self.cancel_pending_after_fault()?;
            return Err(error);
        }
        let outputs = if let Some(image) = &self.image {
            match self.device.process(image, inputs) {
                Ok(v) => v,
                Err(e) => {
                    self.fault = Some(e);
                    self.cancel_pending_after_fault()?;
                    return Err(e);
                }
            }
        } else {
            vec![vec![0; 32]]
        };
        if outputs.len() != 1 || outputs[0].len() != 32 {
            self.fault = Some(Error::InvalidShape);
            return Err(Error::InvalidShape);
        }
        self.frame = next;
        self.ingress
            .publish(Diagnostics {
                frame: self.frame,
                epoch: self.epoch,
                ..Default::default()
            })
            .ok();
        Ok(outputs)
    }
}
