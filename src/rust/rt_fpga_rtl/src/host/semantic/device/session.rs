use super::{
    Device, DeviceRecovery, Error, PublicationStep, RegisterBus, ResultStep, service::Service,
};
use crate::{
    host::{
        RecoveryState, UploadStep,
        control::{ActiveProgram, Session as Core},
        semantic::{
            CommandRecord, GroupHeader, GroupIdentity, GroupPacket, Preparation, PreparationStep,
            SwitchOutcome,
        },
        semantic_registers::*,
    },
    target::{RtlTarget, Space},
};
use rt_fpga_compiler::{
    ProgramLimits,
    loading::{AllocationLease, AllocationOwner, Allocator, Layout, LoadError},
};
use shoop_rt_protocol::immediate::{Code, SwitchProgram};

pub type ProgramSession<B> = Core<Device<B>, Preparation, AllocationOwnership>;
pub type ProgramRecovery<B> = Core<DeviceRecovery<B>, Preparation, AllocationOwnership>;

pub struct AllocationOwnership {
    epoch: u32,
    owner: AllocationOwner,
    lease: Option<AllocationLease>,
    service: Option<Service>,
    enabling: bool,
    next_tag: u32,
    cancel: bool,
    activation: Option<Activation>,
    audio_started: bool,
}
struct Activation {
    identity: GroupIdentity,
    requested: SwitchProgram,
    expected: u64,
    published: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparationProgress {
    Idle,
    Progress,
    Prepared(SwitchProgram),
    Cancelled(u64),
}
pub enum SelectionStep {
    Idle,
    Progress,
    Published(GroupIdentity),
    Fenced {
        selected: ActiveProgram,
        retired: Option<Preparation>,
    },
    Refused {
        code: Code,
        prepared: Preparation,
    },
}
fn limits() -> ProgramLimits {
    ProgramLimits {
        instructions: 1024,
        state_values: 512,
        loop_slots: 128,
        jobs: 512,
        buffers: 512,
        history_slots: 512,
        input_ports: 1,
        output_ports: 2,
        quantum: 32,
    }
}
impl<B: RegisterBus> Device<B> {
    /// Consume a correlated reset incarnation. Discovery alone cannot grant preparation leases.
    pub fn program_session(mut self) -> Result<(Allocator, ProgramSession<B>), Self> {
        if self.reset_epoch != Some(self.epoch())
            || self.halted
            || self.pending.is_some()
            || self.next != 1
            || self.published.iter().any(Option::is_some)
            || self.result.is_some()
            || self.completed.is_some()
        {
            return Err(self);
        }
        self.reset_epoch = None;
        let (allocator, owner) = Allocator::with_leases(limits());
        let authority = AllocationOwnership {
            epoch: self.epoch(),
            owner,
            lease: None,
            service: None,
            enabling: false,
            next_tag: 1,
            cancel: false,
            activation: None,
            audio_started: false,
        };
        Ok((allocator, Core::with_transport(self, authority)))
    }
}
impl<B: RegisterBus> ProgramSession<B> {
    pub fn epoch(&self) -> u32 {
        self.authority.epoch
    }
    pub fn active(&self) -> Option<&crate::target::ProgramImage> {
        self.active.as_ref().map(Preparation::image)
    }
    pub fn staged(&self) -> Option<&crate::target::ProgramImage> {
        self.staged.as_ref().map(Preparation::image)
    }
    /// Lower only the allocator's retained immutable source, never caller-supplied image bytes.
    pub fn stage(&mut self, layout: Layout) -> Result<(), LoadError> {
        if self.failed || self.transport.epoch() != self.epoch() {
            return Err(LoadError::IncompatibleState);
        }
        if self.staged.is_some() || self.authority.activation.is_some() {
            return Err(LoadError::Busy);
        }
        let mut lease = self
            .authority
            .owner
            .claim(self.active().map(|i| &i.layout), &layout)?;
        let program = lease.program();
        let views = layout.media_views(program);
        let image = RtlTarget {
            capacities: limits(),
            media_views: &views,
            state_base: 0,
        }
        .build_semantic_image(program, layout)
        .and_then(Preparation::new);
        let prepared = match image {
            Ok(p) => p,
            Err(error) => {
                lease.cancel()?;
                return Err(error.into());
            }
        };
        if self.stage_prepared(prepared, Preparation::image).is_err() {
            lease.cancel()?;
            return Err(LoadError::Busy);
        }
        self.authority.lease = Some(lease);
        self.authority.cancel = false;
        Ok(())
    }
    fn reserve_service(&mut self, operation: u32, words: [u32; 12]) -> Result<(), Error<B::Error>> {
        if self.authority.next_tag == 0 || self.authority.service.is_some() {
            return Err(Error::Full);
        }
        let tag = self.authority.next_tag;
        self.authority.service = Some(Service::new(operation, words, tag));
        self.authority.next_tag = tag.checked_add(1).unwrap_or(0);
        Ok(())
    }
    pub fn cancel_preparation(&mut self, revision: u64) -> Result<(), Error<B::Error>> {
        if self.failed {
            return Err(Error::Fault);
        }
        if self.authority.activation.is_some()
            || !self.staged().is_some_and(|p| p.layout.revision == revision)
        {
            return Err(Error::InvalidRequest);
        }
        self.authority.cancel = true;
        Ok(())
    }
    /// At most three ordered bus operations; recipe progress is retained until confirmed delivery.
    pub fn preparation_step(&mut self) -> Result<PreparationProgress, Error<B::Error>> {
        if self.failed {
            return Err(Error::Fault);
        }
        let result = self.advance_preparation();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn advance_preparation(&mut self) -> Result<PreparationProgress, Error<B::Error>> {
        if self.authority.activation.is_some() || self.authority.enabling {
            return Ok(PreparationProgress::Idle);
        }
        if let Some(service) = self.authority.service.as_mut() {
            if service.step(&mut self.transport)? {
                self.authority.service = None;
            }
            return Ok(PreparationProgress::Progress);
        }
        if self.authority.cancel {
            self.authority
                .lease
                .as_mut()
                .ok_or(Error::UnexpectedResponse)?
                .cancel()
                .map_err(|_| Error::UnexpectedResponse)?;
            let revision = self
                .staged
                .take()
                .ok_or(Error::UnexpectedResponse)?
                .image()
                .layout
                .revision;
            self.authority.lease = None;
            self.authority.cancel = false;
            self.uploaded = false;
            return Ok(PreparationProgress::Cancelled(revision));
        }
        let Some(prepared) = self.staged.as_mut() else {
            return Ok(PreparationProgress::Idle);
        };
        if self.uploaded {
            return Ok(PreparationProgress::Prepared(prepared.descriptor()));
        }
        if self.authority.next_tag == 0 {
            return Err(Error::Full);
        }
        let mut words = [0; 12];
        let operation = match prepared.next_step() {
            None => {
                self.uploaded = true;
                return Ok(PreparationProgress::Prepared(prepared.descriptor()));
            }
            Some(PreparationStep::Observation { slot, word, value }) => {
                words[0] = 0x103;
                words[1] = u32::from(slot) * 3 + u32::from(word);
                words[2] = value as u32;
                words[3] = (value >> 32) as u32;
                SEMANTIC_SERVICE_CORE_CONFIG
            }
            Some(PreparationStep::Upload(UploadStep::ClearView(view))) => {
                words[0] = view;
                SEMANTIC_SERVICE_CLEAR_VIEW
            }
            Some(PreparationStep::Upload(UploadStep::Configure(write))) => {
                let (operation, space) = match write.space {
                    Space::Program => (SEMANTIC_SERVICE_CORE_CONFIG, 0),
                    Space::State => (SEMANTIC_SERVICE_CORE_CONFIG, 1),
                    Space::Loop => (SEMANTIC_SERVICE_CORE_CONFIG, 2),
                    Space::History => (SEMANTIC_SERVICE_AUDIO_CONFIG, 3),
                };
                words[0] = 0x100 | space;
                words[1] = u32::try_from(write.address).map_err(|_| Error::InvalidRequest)?;
                words[2..7].copy_from_slice(&write.value.array::<5>());
                operation
            }
        };
        self.reserve_service(operation, words)?;
        Ok(PreparationProgress::Progress)
    }
    pub fn select(&mut self, deadline: Option<u64>) -> Result<GroupIdentity, Error<B::Error>> {
        if self.failed {
            return Err(Error::Fault);
        }
        if !self.uploaded
            || self.authority.service.is_some()
            || self.authority.cancel
            || self.authority.activation.is_some()
        {
            return Err(Error::InvalidRequest);
        }
        let image = self.staged().ok_or(Error::InvalidRequest)?;
        let expected = image.layout.expected;
        let requested = crate::host::control::descriptor(image);
        let packet = GroupPacket::new(
            GroupHeader {
                epoch: self.epoch(),
                sequence: self.transport.next_sequence(),
                graph: expected,
                topology: expected,
                deadline,
            },
            &[CommandRecord::switch(
                requested,
                image.layout.has_composites(),
            )],
        )
        .map_err(|_| Error::InvalidRequest)?;
        let identity = packet.identity();
        let lease = self
            .authority
            .lease
            .as_mut()
            .ok_or(Error::UnexpectedResponse)?;
        lease.publishing().map_err(|_| Error::UnexpectedResponse)?;
        if let Err(error) = self.transport.queue(packet) {
            lease.unpublished().map_err(|_| Error::UnexpectedResponse)?;
            return Err(error);
        }
        self.authority.activation = Some(Activation {
            identity,
            requested,
            expected,
            published: false,
        });
        Ok(identity)
    }
    /// Initial publication precedes enable. Only real audio selects a processing boundary.
    pub fn selection_step(&mut self) -> Result<SelectionStep, Error<B::Error>> {
        if self.failed {
            return Err(Error::Fault);
        }
        let result = self.advance_selection();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn advance_selection(&mut self) -> Result<SelectionStep, Error<B::Error>> {
        let Some(activation) = self.authority.activation.as_mut() else {
            return Ok(SelectionStep::Idle);
        };
        if !activation.published {
            if let PublicationStep::Published { identity, .. } =
                self.transport.publication_step()?
            {
                if identity != activation.identity {
                    return Err(Error::UnexpectedResponse);
                }
                activation.published = true;
                return Ok(SelectionStep::Published(identity));
            }
            return Ok(SelectionStep::Progress);
        }
        if !self.authority.audio_started {
            if let Some(service) = self.authority.service.as_mut() {
                if service.step(&mut self.transport)? {
                    self.authority.service = None;
                    self.authority.enabling = false;
                    self.authority.audio_started = true;
                }
            } else {
                let mut words = [0; 12];
                words[0] = 1;
                self.reserve_service(SEMANTIC_SERVICE_ENABLE_AUDIO, words)?;
                self.authority.enabling = true;
            }
            return Ok(SelectionStep::Progress);
        }
        if self.transport.result_step()? != ResultStep::Buffered {
            return Ok(SelectionStep::Progress);
        }
        let result = self
            .transport
            .take_result()
            .ok_or(Error::UnexpectedResponse)?;
        let header = result.header();
        let activation = self.authority.activation.as_ref().unwrap();
        if header.identity != activation.identity
            || header.faulted
            || header.initial_revision != activation.expected
        {
            return Err(Error::UnexpectedResponse);
        }
        let lease = self
            .authority
            .lease
            .as_mut()
            .ok_or(Error::UnexpectedResponse)?;
        if header.switch == SwitchOutcome::Fenced {
            if header.current_revision != activation.requested.revision
                || result.codes() != [Code::Applied]
            {
                return Err(Error::UnexpectedResponse);
            }
            lease.fenced().map_err(|_| Error::UnexpectedResponse)?;
            let (selected, retired) = self.accept_selection(
                activation.requested,
                ActiveProgram {
                    revision: header.current_revision,
                    frame: header.first_frame,
                },
                Preparation::image,
            )?;
            self.authority.activation = None;
            self.authority.lease = None;
            Ok(SelectionStep::Fenced { selected, retired })
        } else {
            if header.current_revision != activation.expected
                || !matches!(header.switch, SwitchOutcome::None | SwitchOutcome::Refused)
            {
                return Err(Error::UnexpectedResponse);
            }
            lease.refused().map_err(|_| Error::UnexpectedResponse)?;
            let code = result.codes()[0];
            let prepared = self.staged.take().ok_or(Error::UnexpectedResponse)?;
            self.uploaded = false;
            self.authority.activation = None;
            self.authority.lease = None;
            Ok(SelectionStep::Refused { code, prepared })
        }
    }
}
impl<B: RegisterBus + Clone> ProgramSession<B> {
    pub fn recovery(self) -> ProgramRecovery<B> {
        Core {
            transport: self.transport.recovery(),
            active: self.active,
            staged: self.staged,
            uploaded: self.uploaded,
            failed: self.failed,
            authority: self.authority,
        }
    }
}
impl<B: RegisterBus + Clone> ProgramRecovery<B> {
    pub fn begin(&mut self) -> Result<(), Error<B::Error>> {
        self.transport.begin()
    }
    pub fn poll(&mut self) -> Result<RecoveryState, Error<B::Error>> {
        self.transport.poll()
    }
    pub fn reopen(&mut self) -> Result<(Allocator, ProgramSession<B>), Error<B::Error>> {
        if self.transport.completed.is_none() {
            return Err(Error::Full);
        }
        let device = self.transport.reopen()?;
        let fresh = device
            .program_session()
            .map_err(|_| Error::UnexpectedResponse)?;
        self.active = None;
        self.staged = None;
        Ok(fresh)
    }
}
