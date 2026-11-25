use super::{
    serve, shoop_delay, shoop_millis, shoop_timer_hz, shoop_timer_ticks, Clock, TerminalLog,
};
use crate::runtime_restart::Renderer;
use alloc::{boxed::Box, sync::Arc, vec};
use core::{
    ffi::c_void,
    fmt::{Debug, Write},
    sync::atomic::{AtomicU8, Ordering},
};
use rt_fpga_rtl::host::{Mailbox, Recovery, RecoveryState, RegisterBus};
use shoop_app::{Service, ServiceOptions};
use shoop_app_api::{
    AudioDriverConfig, BackendType, HostResourceLimits, HostSettings, NewEnginePools,
};
use shoop_app_protocol::BackendRestartFailure as Failure;
use shoop_rt_backend::RtBackend;
use shoop_rt_host_reference::{fpga, program_limits};
use shoop_rt_protocol::{DriverKind, DriverRunState, DriverStatus};
use shoop_rt_transport::{
    carrier::{InProcess, Outbox},
    client::ProtocolClient,
};

extern "C" {
    fn shoop_start_fpga_task(
        entry: unsafe extern "C" fn(*mut c_void) -> i32,
        owner: *mut c_void,
    ) -> i32;
    fn shoop_fpga_irq_start() -> i32;
    fn shoop_fpga_irq_wait() -> i32;
    fn shoop_fpga_irq_stop();
    fn shoop_fpga_stop_requested() -> i32;
    fn shoop_fpga_task_progress();
    fn shoop_stop_fpga_task() -> i32;
}

struct Task<B> {
    owner: fpga::Owner<B>,
    startup: Arc<AtomicU8>,
}

fn budget(millis: u64) -> impl FnMut() -> bool {
    let started = unsafe { shoop_timer_ticks() };
    let ticks = unsafe { shoop_timer_hz() } / 1000 * millis;
    move || unsafe { shoop_timer_ticks() }.wrapping_sub(started) < ticks
}

unsafe extern "C" fn task<B: RegisterBus>(handle: *mut c_void) -> i32
where
    B::Error: Debug,
{
    let task = unsafe { &mut *handle.cast::<Task<B>>() };
    if unsafe { shoop_fpga_irq_start() } != 1 {
        unsafe { shoop_fpga_irq_stop() };
        task.startup.store(2, Ordering::Release);
        return -1;
    }
    let result = (|| {
        task.owner.initialize(budget(400))?;
        let _ = writeln!(TerminalLog, "SHOOP_FPGA_OWNER_READY");
        task.startup.store(1, Ordering::Release);
        let mut burst = 0;
        while unsafe { shoop_fpga_stop_requested() } == 0 {
            task.owner.step(budget(25))?;
            unsafe {
                shoop_fpga_task_progress();
                if shoop_fpga_irq_wait() == 0 {
                    burst = 0;
                } else {
                    burst += 1;
                    if burst == 4 {
                        shoop_delay(1);
                        burst = 0;
                    }
                }
            }
        }
        task.owner.stop(budget(250))
    })();
    unsafe { shoop_fpga_irq_stop() };
    if let Err(error) = result {
        let _ = writeln!(
            TerminalLog,
            "SHOOP_FPGA_FIRST_FAULT {:?}",
            task.owner.fault_record()
        );
        task.owner.fail();
        task.startup.store(2, Ordering::Release);
        let _ = writeln!(TerminalLog, "SHOOP_FPGA_OWNER_FAILED {error}");
        return -1;
    }
    1
}

fn reap<B>(handle: &mut Option<*mut Task<B>>, cleaned: &mut bool) -> Option<bool> {
    let Some(raw) = *handle else {
        return Some(*cleaned);
    };
    match unsafe { shoop_stop_fpga_task() } {
        2 => None,
        result => {
            // Failed owners remain allocated without a fabric fence.
            if result == 1 {
                drop(unsafe { Box::from_raw(raw) });
            }
            if result != 0 {
                *handle = None;
            }
            *cleaned = result == 1;
            Some(*cleaned)
        }
    }
}

pub fn run<B: RegisterBus + Clone + Send + 'static>(mailbox: Mailbox<B>)
where
    B::Error: Debug,
{
    let recovery = mailbox.recovery();
    let driver = DriverStatus {
        kind: DriverKind::Simulated,
        state: DriverRunState::Starting,
        sample_rate: crate::SAMPLE_RATE,
        buffer_frames: crate::PROCESSING_QUANTUM as u32,
    };
    let (endpoint, owner) = match fpga::prepare(mailbox, driver) {
        Ok(prepared) => prepared,
        Err(error) => {
            let _ = writeln!(TerminalLog, "SHOOP_FPGA_HOST_UNAVAILABLE {error}");
            return;
        }
    };
    let startup = Arc::new(AtomicU8::new(0));
    let raw = Box::into_raw(Box::new(Task {
        owner,
        startup: startup.clone(),
    }));
    if unsafe { shoop_start_fpga_task(task::<B>, raw.cast()) } != 1 {
        drop(unsafe { Box::from_raw(raw) });
        let _ = writeln!(TerminalLog, "SHOOP_FPGA_HOST_UNAVAILABLE owner task");
        return;
    }
    let mut handle = Some(raw);
    let mut cleaned = false;
    let started = unsafe { shoop_millis() };
    while startup.load(Ordering::Acquire) == 0
        && unsafe { shoop_millis() }.wrapping_sub(started) < 1000
    {
        unsafe { shoop_delay(1) };
    }
    if startup.load(Ordering::Acquire) != 1 {
        let _ = writeln!(TerminalLog, "SHOOP_FPGA_HOST_UNAVAILABLE owner startup");
        while reap(&mut handle, &mut cleaned).is_none() {
            unsafe { shoop_delay(1) };
        }
        return;
    }

    let status = endpoint.host_status();
    let pools = program_limits();
    let limits = HostResourceLimits {
        media_bytes: crate::MEDIA_BYTES,
        metadata_bytes: 1024 * 1024,
        objects: 512,
        midi_events: 65_536,
    };
    let outbox = Outbox::new(520, 2 * pools.loop_slots as usize + 8, 8);
    let backend = RtBackend::connect(ProtocolClient::new(InProcess::new(endpoint, outbox)), Clock)
        .expect("FPGA backend connection")
        .with_resource_limits(limits)
        .expect("FPGA backend limits");
    let service = make_service(backend).expect("FPGA service");
    serve(
        service,
        FpgaRuntime {
            handle,
            cleaned,
            recovery,
            worker_stopped: false,
            resetting: false,
            backend: None,
            status,
            startup,
        },
    );
}

type Backend = RtBackend<InProcess<shoop_rt_host_reference::Endpoint<fpga::Images>>, Clock>;

fn make_service(backend: Backend) -> Result<Service, Failure> {
    let pools = program_limits();
    let limits = HostResourceLimits {
        media_bytes: crate::MEDIA_BYTES,
        metadata_bytes: 1024 * 1024,
        objects: 512,
        midi_events: 65_536,
    };
    let driver = DriverStatus {
        kind: DriverKind::Simulated,
        state: DriverRunState::Starting,
        sample_rate: crate::SAMPLE_RATE,
        buffer_frames: crate::PROCESSING_QUANTUM as u32,
    };
    let mut service = Service::new_with_resource_limits(
        Box::new(backend),
        ServiceOptions {
            master_auto_connect: true,
            loop_smoothing_ms: 0,
        },
        limits,
    )
    .map_err(|_| Failure::InitializationFailed)?;
    service
        .set_host_settings(
            HostSettings {
                backend: BackendType::RtlSim,
                processors: shoop_app_api::ProcessorSettings::disabled(),
                driver: AudioDriverConfig::Simulated,
                pools: NewEnginePools {
                    quantum: driver.buffer_frames,
                    buses: 1,
                    bus_channels: 2,
                    tracks: 8,
                    loops: pools.loop_slots,
                    channels: 2,
                    loop_memory_mb: 124,
                },
                master_auto_connect: true,
                loop_smoothing_ms: 0,
            },
            vec!["Zynq FPGA renderer at 48 kHz with FreeRTOS and AMD lwIP".into()],
        )
        .map_err(|_| Failure::InitializationFailed)?;
    Ok(service)
}

struct FpgaRuntime<B> {
    handle: Option<*mut Task<B>>,
    cleaned: bool,
    recovery: Recovery<B>,
    worker_stopped: bool,
    resetting: bool,
    backend: Option<Backend>,
    status: Arc<shoop_rt_host_reference::Status>,
    startup: Arc<AtomicU8>,
}

impl<B: RegisterBus + Clone + Send + 'static> Renderer for FpgaRuntime<B>
where
    B::Error: Debug,
{
    fn cleanup(&mut self) -> Option<bool> {
        if !self.worker_stopped {
            match unsafe { shoop_stop_fpga_task() } {
                2 => return None,
                0 => return Some(false),
                result => {
                    self.worker_stopped = true;
                    self.cleaned = result == 1;
                    if self.cleaned {
                        if let Some(raw) = self.handle.take() {
                            drop(unsafe { Box::from_raw(raw) });
                        }
                    }
                }
            }
        }
        Some(self.cleaned)
    }

    fn restart_supported(&mut self) -> bool {
        self.recovery.supported().unwrap_or(false)
    }

    fn stop_for_restart(&mut self) -> Result<bool, Failure> {
        if self.worker_stopped {
            return Ok(true);
        }
        match unsafe { shoop_stop_fpga_task() } {
            2 => Ok(false),
            0 => Err(Failure::WorkerDidNotStop),
            _ => {
                self.worker_stopped = true;
                Ok(true)
            }
        }
    }

    fn reset(&mut self) -> Result<bool, Failure> {
        if !self.worker_stopped {
            return Err(Failure::WorkerDidNotStop);
        }
        if !self.resetting {
            self.recovery
                .begin()
                .map_err(|_| Failure::FabricDidNotQuiesce)?;
            self.resetting = true;
        }
        match self
            .recovery
            .poll()
            .map_err(|_| Failure::FabricDidNotQuiesce)?
        {
            RecoveryState::Draining | RecoveryState::Resetting => Ok(false),
            RecoveryState::Failed => Err(Failure::FabricDidNotQuiesce),
            RecoveryState::Ready(_) => {
                if let Some(raw) = self.handle.take() {
                    drop(unsafe { Box::from_raw(raw) });
                }
                self.resetting = false;
                Ok(true)
            }
        }
    }

    fn start(&mut self) -> Result<Option<Service>, Failure> {
        if self.backend.is_none() {
            let mailbox = self
                .recovery
                .reopen()
                .map_err(|_| Failure::InitializationFailed)?;
            let driver = DriverStatus {
                kind: DriverKind::Simulated,
                state: DriverRunState::Starting,
                sample_rate: crate::SAMPLE_RATE,
                buffer_frames: crate::PROCESSING_QUANTUM as u32,
            };
            let (endpoint, owner) =
                fpga::prepare(mailbox, driver).map_err(|_| Failure::InitializationFailed)?;
            self.status = endpoint.host_status();
            self.startup = Arc::new(AtomicU8::new(0));
            let raw = Box::into_raw(Box::new(Task {
                owner,
                startup: self.startup.clone(),
            }));
            if unsafe { shoop_start_fpga_task(task::<B>, raw.cast()) } != 1 {
                drop(unsafe { Box::from_raw(raw) });
                return Err(Failure::InitializationFailed);
            }
            self.handle = Some(raw);
            self.worker_stopped = false;
            self.cleaned = false;
            let pools = program_limits();
            let backend = RtBackend::begin(
                ProtocolClient::new(InProcess::new(
                    endpoint,
                    Outbox::new(520, 2 * pools.loop_slots as usize + 8, 8),
                )),
                Clock,
            )
            .map_err(|_| Failure::InitializationFailed)?;
            self.backend = Some(
                backend
                    .with_resource_limits(HostResourceLimits {
                        media_bytes: crate::MEDIA_BYTES,
                        metadata_bytes: 1024 * 1024,
                        objects: 512,
                        midi_events: 65_536,
                    })
                    .map_err(|_| Failure::InitializationFailed)?,
            );
        }
        if self.startup.load(Ordering::Acquire) == 2 {
            return Err(Failure::InitializationFailed);
        }
        let backend = self.backend.as_mut().expect("connecting backend");
        backend.pump();
        if !backend.is_ready() {
            return Ok(None);
        }
        make_service(self.backend.take().unwrap()).map(Some)
    }

    fn progressed(&self) -> bool {
        self.status.frame() > 0 && self.startup.load(Ordering::Acquire) == 1
    }

    fn quarantine(&mut self) {
        if !self.worker_stopped && self.handle.is_some() {
            let _ = self.stop_for_restart();
        }
    }
}
