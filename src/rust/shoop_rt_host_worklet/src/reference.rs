//! Main-thread compiler and bounded, owned image transfer between separate Wasm heaps.
use anyhow::{ensure, Result};
use rt_fpga_compiler::{loading::Layout, LogicalStateProgram};
use rt_fpga_reference::{
    rt_interface::PreparedProgram,
    storage::ViewId,
    target::{ProgramImage, RustTarget},
};
use shoop_rt_host_reference::{ProgramLoader, Status};
use shoop_rt_protocol::{
    immediate::{Batch, Command, Response, SwitchProgram},
    snapshot, ClientMessage, DriverRunState, DriverStatus, EngineMessage, Telemetry,
};
use shoop_rt_transport::{
    carrier::{CarrierError, Endpoint as Carrier, Outbox, RecordClient, RecordPort},
    client::{ClientState, ProtocolClient},
    rt,
};
use std::sync::Arc;
pub const MAX_IMAGE_BYTES: usize = 1024 * 1024;
pub trait ImagePort: RecordPort + Clone {
    fn upload(&mut self, bytes: &[u8]) -> Result<(), CarrierError>;
    fn cancel_image(&mut self, revision: u64) -> Result<(), CarrierError>;
    fn image_ready(&mut self) -> Option<(u64, bool)>;
}
pub struct Images<P> {
    port: P,
}
impl<P: ImagePort> ProgramLoader for Images<P> {
    fn limits(&self) -> rt_fpga_compiler::ProgramLimits {
        shoop_rt_host_reference::program_limits()
    }
    fn stage_program(
        &mut self,
        logical: &LogicalStateProgram,
        layout: Layout,
    ) -> Result<SwitchProgram> {
        let views = layout.media_views(logical);
        let mut target = RustTarget::new(&views);
        target.limits = self.limits();
        let image = target.build_image(logical, layout)?;
        let bytes = serde_json::to_vec(&image)?;
        ensure!(
            bytes.len() <= MAX_IMAGE_BYTES,
            "worklet image exceeds prepared transfer limit"
        );
        let unused = image
            .layout
            .new_media_views()
            .map(|view| ViewId(view as usize))
            .collect();
        let prepared = PreparedProgram::new(image, unused).map_err(|e| anyhow::anyhow!("{e:?}"))?;
        let descriptor = prepared.descriptor();
        self.port
            .upload(&bytes)
            .map_err(|e| anyhow::anyhow!("worklet image delivery: {e:?}"))?;
        Ok(descriptor)
    }
    fn program_ready(&mut self) -> Option<(u64, bool)> {
        self.port.image_ready()
    }
    fn cancel_program(&mut self, revision: u64) -> Result<(), CarrierError> {
        self.port.cancel_image(revision)
    }
    fn reclaim_programs(&mut self) {}
}
pub fn decode_image(bytes: &[u8]) -> Result<Box<PreparedProgram>> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_IMAGE_BYTES,
        "invalid worklet image size"
    );
    let image: ProgramImage = serde_json::from_slice(bytes)?;
    let unused = image
        .layout
        .new_media_views()
        .map(|view| ViewId(view as usize))
        .collect();
    PreparedProgram::new(image, unused).map_err(|e| anyhow::anyhow!("worklet image: {e:?}"))
}
#[derive(Default)]
struct ForwardedGroup {
    inflight: bool,
    response: Option<Response>,
}

impl ForwardedGroup {
    fn service<P: ImagePort>(
        &mut self,
        queue: &mut rt::CommandConsumer,
        remote: &mut ProtocolClient<RecordClient<P>>,
    ) {
        queue.begin_callback();
        if let Some(response) = self.response.take() {
            let group = queue.pop().expect("forwarded command group");
            let mut records = response.records().iter().copied();
            group.respond(response.header, |_| {
                records.next().expect("forwarded response record")
            });
            self.inflight = false;
        }
        if !self.inflight && remote.state() == ClientState::Connected {
            if let Some(group) = queue.pop() {
                let commands = group.commands().map(|r| r.command()).collect::<Vec<_>>();
                let mut batch =
                    Batch::new(group.header.graph(), group.header.topology(), &commands).unwrap();
                batch.deadline = group.header.deadline();
                self.inflight = remote.immediate(batch).is_ok();
                group.leave_unread();
            }
        }
    }
}

pub struct Endpoint<P: ImagePort> {
    endpoint: shoop_rt_host_reference::Endpoint<Images<P>>,
    execution: rt::Execution,
    remote: ProtocolClient<RecordClient<P>>,
    commands: ForwardedGroup,
    switches: ForwardedGroup,
    snapshot: Vec<u8>,
    sequence: u32,
    offset: usize,
    status: Arc<Status>,
    quantum: u32,
}
impl<P: ImagePort> Endpoint<P> {
    pub fn new(port: P, quantum: u32, driver: DriverStatus) -> Result<Self> {
        let (control, mut execution) =
            shoop_rt_host_reference::prepare_control(quantum, driver, quantum)?;
        let status = Arc::new(Status::new(driver));
        let endpoint = shoop_rt_host_reference::Endpoint::new(
            control,
            Images { port: port.clone() },
            quantum,
            status.clone(),
        )
        .with_reference_bootstrap();
        let mut snapshot = vec![
            0;
            snapshot::capacity(
                0,
                shoop_rt_host_reference::program_limits().loop_slots as usize,
                0
            )
            .unwrap()
        ];
        snapshot::Header {
            frame: 0,
            graph: 0,
            topology: 0,
            records: 0,
            loops: 0,
            strips: 0,
            buses: 0,
        }
        .write(&mut snapshot);
        execution.snapshots.publish(&snapshot[..snapshot::HEADER])?;
        let mut remote = ProtocolClient::new(RecordClient::new(port));
        remote
            .connect()
            .map_err(|e| anyhow::anyhow!("worklet connect: {e:?}"))?;
        Ok(Self {
            endpoint,
            execution,
            remote,
            commands: ForwardedGroup::default(),
            switches: ForwardedGroup::default(),
            snapshot,
            sequence: 0,
            offset: 0,
            status,
            quantum,
        })
    }
    fn service(&mut self) {
        for _ in 0..32 {
            let Some(message) = self.remote.poll() else {
                break;
            };
            match message {
                EngineMessage::Welcome { driver, .. } => {
                    self.status.update_driver(driver);
                    let _ = self.remote.observe_snapshots(true);
                }
                EngineMessage::ImmediateResponse(response) => {
                    if matches!(response.records(), [record] if matches!(record.command(), Command::SwitchProgram(_)))
                    {
                        self.switches.response = Some(response);
                    } else {
                        self.commands.response = Some(response);
                    }
                }
                EngineMessage::Telemetry(Telemetry::Engine {
                    driver,
                    callbacks,
                    xruns,
                    dsp,
                    ..
                }) => self
                    .status
                    .observe_worklet_engine(driver, callbacks, xruns, dsp),
                EngineMessage::Telemetry(Telemetry::Memory(report)) => {
                    self.endpoint.observe_worklet_storage(&report);
                }
                EngineMessage::Snapshot(chunk) | EngineMessage::SnapshotReply(chunk) => {
                    let total = chunk.total as usize;
                    let offset = chunk.offset as usize;
                    let len = usize::from(chunk.len);
                    if offset == 0 {
                        self.offset = 0;
                        self.sequence = chunk.sequence;
                    }
                    if total > self.snapshot.len()
                        || offset != self.offset
                        || chunk.sequence != self.sequence
                        || offset + len > total
                    {
                        self.offset = 0;
                        continue;
                    }
                    self.snapshot[offset..offset + len].copy_from_slice(&chunk.bytes[..len]);
                    self.offset += len;
                    if self.offset == total {
                        if let Some(header) = snapshot::Header::read(&self.snapshot[..total]) {
                            self.execution.commands.publish_frame(rt_runtime::Frame(
                                header.frame + u64::from(self.quantum),
                            ));
                            self.status
                                .set_frame(header.frame + u64::from(self.quantum));
                            let _ = self.execution.snapshots.publish(&self.snapshot[..total]);
                        }
                    }
                }
                EngineMessage::Disconnected(_) | EngineMessage::Rejected { .. } => {
                    let mut d = self.status.driver();
                    d.state = DriverRunState::Failed;
                    self.status.update_driver(d);
                }
                _ => {}
            }
        }
        self.commands
            .service(&mut self.execution.commands, &mut self.remote);
        self.switches.service(
            self.execution
                .switches
                .as_mut()
                .expect("prepared switch lane"),
            &mut self.remote,
        );
    }
}
impl<P: ImagePort> Carrier for Endpoint<P> {
    fn try_deliver(
        &mut self,
        message: ClientMessage,
        out: &mut Outbox,
    ) -> Result<(), CarrierError> {
        self.service();
        self.endpoint.try_deliver(message, out)
    }
    fn deliver(&mut self, message: ClientMessage, out: &mut Outbox) {
        let _ = self.try_deliver(message, out);
    }
    fn poll(&mut self, out: &mut Outbox) {
        self.service();
        self.endpoint.poll(out);
        self.service();
    }
}
