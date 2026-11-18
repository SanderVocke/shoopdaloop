use super::{
    serve, shoop_delay, shoop_millis, shoop_timer_hz, shoop_timer_ticks, Clock, TerminalLog,
};
use alloc::{boxed::Box, sync::Arc, vec};
use core::{
    ffi::c_void,
    fmt::{Debug, Write},
    sync::atomic::{AtomicU8, Ordering},
};
use rt_fpga_rtl::host::{Mailbox, RegisterBus};
use shoop_app::{Service, ServiceOptions};
use shoop_app_api::{
    AudioDriverConfig, BackendType, HostResourceLimits, HostSettings, NewEnginePools,
};
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
            if result != 0 {
                drop(unsafe { Box::from_raw(raw) });
                *handle = None;
            }
            *cleaned = result == 1;
            Some(*cleaned)
        }
    }
}

pub fn run<B: RegisterBus + Send + 'static>(mailbox: Mailbox<B>)
where
    B::Error: Debug,
{
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
    let mut service = Service::new_with_resource_limits(
        Box::new(backend),
        ServiceOptions {
            master_auto_connect: true,
            loop_smoothing_ms: 0,
        },
        limits,
    )
    .expect("FPGA service");
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
        .expect("FPGA host settings");
    serve(service, || reap(&mut handle, &mut cleaned));
}
