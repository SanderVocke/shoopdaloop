use alloc::{boxed::Box, string::String, vec, vec::Vec};
use core::{ffi::c_void, time::Duration};
use rt_runtime::buffer::{AudioBlock, MidiBlock, MidiBuffer};
use shoop_app::{ConnectionId, Service, ServiceOptions};
use shoop_app_api::{
    AudioDriverConfig, DummyAudioDriverConfig, HostResourceLimits, HostSettings, NewEnginePools,
};
use shoop_app_connection::{HeartbeatConfig, Management, Session};
use shoop_app_protocol::{codec, WireMessage};
use shoop_rt_backend::{EngineClock, RtBackend};
use shoop_rt_engine::{prepare, EngineDescription, HostPorts, HostStatus, Phase, Render, Segment};
use shoop_rt_protocol::{DriverKind, DriverRunState, DriverStatus};
use shoop_rt_transport::{carrier::InProcess, client::ProtocolClient};
use shoop_transport::{
    authenticated::Handshake,
    stream::{ByteIo, Config, IoError, Stream},
    RecordPort, MAX_RECORD_BYTES,
};

extern "C" {
    fn shoop_listen(port: u16) -> i32;
    fn shoop_accept(listener: i32) -> i32;
    fn shoop_read(fd: i32, bytes: *mut u8, len: usize) -> i32;
    fn shoop_write(fd: i32, bytes: *const u8, len: usize) -> i32;
    fn shoop_close(fd: i32);
    fn shoop_millis() -> u64;
    fn shoop_delay(ms: u32);
    fn shoop_start_render(owner: *mut c_void);
    fn shoop_stop_render() -> i32;
    fn shoop_cleanup_allowed() -> i32;
    fn shoop_log(bytes: *const u8, len: usize);
    fn shoop_token(bytes: *mut u8);
    fn shoop_nonce() -> u64;
    fn shoop_boot_nonce() -> u64;
    fn shoop_control_progress();
    fn shoop_deployment_ready() -> i32;
    fn shoop_deployment_info(info: *mut DeploymentInfo) -> i32;
    fn shoop_deployment_request(client: *const u8, sequence: u64) -> i32;
    fn shoop_deployment_reset();
    fn shoop_memory_report();
}

#[cfg(feature = "usb-midi")]
extern "C" {
    fn shoop_usb_micros() -> u64;
    fn shoop_usb_read(slot: u32, bytes: *mut u8, capacity: usize, arrival: *mut u64) -> usize;
    fn shoop_usb_write(slot: u32, bytes: *const u8, len: usize, due: u64) -> bool;
    fn shoop_usb_begin_stop();
    fn shoop_usb_recover_port(slot: u32);
    fn shoop_midi_offset(arrival: u64, now: u64, frames: u32, rate: u32) -> u32;
    fn shoop_usb_port_snapshot(direction: u32, slot: u32, info: *mut UsbPortInfo) -> bool;
    fn shoop_usb_release(direction: u32, slot: u32, generation: u32);
}

#[cfg(feature = "usb-midi")]
#[derive(Clone, Copy, Eq, PartialEq)]
#[repr(C)]
struct UsbPortInfo {
    generation: u32,
    connected: u32,
    direction: u32,
    vendor: u16,
    product: u16,
    slot: u8,
    index: u8,
    address: u8,
    interface_number: u8,
    cable: u8,
    speed: u8,
    reserved: [u8; 2],
    key: [u8; 128],
    name: [u8; 128],
}

#[cfg(feature = "usb-midi")]
const _: () = assert!(core::mem::size_of::<UsbPortInfo>() == 280);

#[cfg(feature = "usb-midi")]
struct UsbBinding {
    direction: u32,
    slot: u32,
    id: String,
    observed: Option<UsbPortInfo>,
}

#[cfg(feature = "usb-midi")]
fn usb_port_info(direction: u32, slot: u32) -> Option<UsbPortInfo> {
    let mut storage = core::mem::MaybeUninit::<UsbPortInfo>::uninit();
    unsafe {
        shoop_usb_port_snapshot(direction, slot, storage.as_mut_ptr())
            .then(|| storage.assume_init())
    }
}

#[cfg(feature = "usb-midi")]
fn usb_text(bytes: &[u8]) -> &str {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    core::str::from_utf8(&bytes[..end]).unwrap_or("")
}

#[cfg(feature = "usb-midi")]
struct UsbInventory(Vec<UsbBinding>);

#[cfg(feature = "usb-midi")]
impl UsbInventory {
    fn new() -> Self {
        Self(
            (0..2)
                .flat_map(|direction| {
                    (0..crate::MIDI_PORTS).map(move |slot| UsbBinding {
                        direction,
                        slot: slot as u32,
                        id: alloc::format!(
                            "midi_{}_{}",
                            if direction == 0 { "in" } else { "out" },
                            slot + 1
                        ),
                        observed: None,
                    })
                })
                .collect(),
        )
    }
}

#[cfg(feature = "usb-midi")]
impl shoop_rt_backend::HostPortInventory for UsbInventory {
    fn poll(&mut self) -> Option<Vec<shoop_rt_backend::HostPortAlias>> {
        let mut changed = false;
        for binding in &mut self.0 {
            if let Some(info) = usb_port_info(binding.direction, binding.slot) {
                if binding.observed != Some(info) {
                    binding.observed = Some(info);
                    changed = true;
                }
            }
        }
        changed.then(|| {
            self.0
                .iter()
                .map(|binding| shoop_rt_backend::HostPortAlias {
                    slot: binding.id.clone(),
                    id: binding
                        .observed
                        .as_ref()
                        .map_or("", |info| usb_text(&info.key))
                        .into(),
                    name: binding
                        .observed
                        .as_ref()
                        .map_or("", |info| usb_text(&info.name))
                        .into(),
                    available: binding.observed.is_some_and(|info| info.connected != 0),
                })
                .collect()
        })
    }
}

#[cfg(feature = "usb-midi")]
fn usb_release_unused(service: &mut Service) {
    for direction in 0..2 {
        for slot in 0..crate::MIDI_PORTS {
            let Some(info) = usb_port_info(direction, slot as u32) else {
                continue;
            };
            if info.connected == 0
                && info.key[0] != 0
                && service.release_host_port_identity(usb_text(&info.key))
            {
                unsafe {
                    shoop_usb_release(direction, slot as u32, info.generation);
                }
            }
        }
    }
}

#[repr(C)]
struct DeploymentInfo {
    enabled: u32,
    slot: u32,
    confirmed: u32,
    trial: u32,
    nonce: u64,
    build: [u8; 64],
    hash: [u8; 32],
    operation_client: [u8; 16],
    operation_sequence: u64,
    profile: [u8; 32],
}
const _: () = assert!(core::mem::size_of::<DeploymentInfo>() == 176);

fn deployment_info() -> Option<shoop_deployment_protocol::Info> {
    let mut info = DeploymentInfo {
        enabled: 0,
        slot: 0,
        confirmed: 0,
        trial: 0,
        nonce: 0,
        build: [0; 64],
        hash: [0; 32],
        operation_client: [0; 16],
        operation_sequence: 0,
        profile: [0; 32],
    };
    if unsafe { shoop_deployment_info(&mut info) } == 0 {
        return None;
    }
    fn text(bytes: &[u8]) -> String {
        let end = bytes
            .iter()
            .position(|b| *b == 0)
            .expect("terminated platform identity");
        String::from_utf8(bytes[..end].to_vec()).expect("ASCII platform identity")
    }
    let client = u128::from_le_bytes(info.operation_client);
    Some(shoop_deployment_protocol::Info {
        profile: text(&info.profile),
        build: text(&info.build),
        hash: info.hash,
        slot: if info.slot < 2 {
            Some(info.slot as u8)
        } else {
            None
        },
        confirmed: info.confirmed != 0,
        trial: info.trial != 0,
        boot_nonce: info.nonce,
        last_operation: if client != 0 && info.operation_sequence != 0 {
            Some(shoop_deployment_protocol::Operation {
                client,
                sequence: info.operation_sequence,
            })
        } else {
            None
        },
        session_version: shoop_app_protocol::VERSION,
        busy: false,
        ready: false,
    })
}

fn log(text: &str) {
    unsafe {
        shoop_log(text.as_ptr(), text.len());
    }
}

struct Socket(i32);
impl Drop for Socket {
    fn drop(&mut self) {
        unsafe {
            shoop_close(self.0);
        }
    }
}
fn io_result(result: i32) -> Result<usize, IoError> {
    match result {
        -2 => Err(IoError::WouldBlock),
        -3 => Err(IoError::Interrupted),
        n if n >= 0 => Ok(n as usize),
        _ => Err(IoError::Disconnected),
    }
}
impl ByteIo for Socket {
    fn try_read(&mut self, output: &mut [u8]) -> Result<usize, IoError> {
        io_result(unsafe { shoop_read(self.0, output.as_mut_ptr(), output.len()) })
    }
    fn try_write(&mut self, input: &[u8]) -> Result<usize, IoError> {
        io_result(unsafe { shoop_write(self.0, input.as_ptr(), input.len()) })
    }
}
struct Clock;
impl EngineClock for Clock {
    fn wait(&mut self) -> bool {
        unsafe {
            shoop_delay(1);
        }
        true
    }
}

struct Dummy {
    render: Render,
    status: HostStatus,
    input: [[f32; crate::QUANTUM]; 2],
    output: [[f32; crate::QUANTUM]; 2],
    midi_in: Vec<MidiBuffer>,
    midi_out: Vec<MidiBuffer>,
    reset: bool,
    #[cfg(feature = "usb-midi")]
    usb_scratch: [u8; 2048],
}

#[cfg(feature = "fixture")]
static FIXTURE_FRAME: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
#[cfg(feature = "fixture")]
static FIXTURE_HASH: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
#[cfg(feature = "fixture")]
static FIXTURE_MIDI: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
#[cfg(feature = "fixture")]
static FIXTURE_ON: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
#[cfg(feature = "fixture")]
static FIXTURE_OFF: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
#[cfg(feature = "fixture")]
static FIXTURE_BAD_NOTES: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

#[cfg(feature = "fixture")]
pub unsafe fn fixture_step(owner: *mut c_void, frames: usize, input: u32, midi: u32) -> i32 {
    {
        let dummy = &mut *owner.cast::<Dummy>();
        let value = f32::from_bits(input);
        assert!(value.is_finite());
        dummy.input[0].fill(value);
        dummy.input[1].fill(-value);
        match midi {
            1 => dummy.midi_in[0].push(0, frames, &[0x90, 60, 100]).unwrap(),
            2 => dummy.midi_in[0].push(0, frames, &[0x80, 60, 0]).unwrap(),
            _ => {}
        }
    }
    let result = render_step(owner, frames);
    let dummy = &*owner.cast::<Dummy>();
    let mut hash = 2_166_136_261u32;
    for channel in &dummy.output {
        for sample in &channel[..frames] {
            hash = hash.wrapping_mul(16_777_619) ^ sample.to_bits();
        }
    }
    use core::sync::atomic::Ordering;
    FIXTURE_FRAME.store(dummy.render.next_frame().0 as u32, Ordering::Release);
    FIXTURE_HASH.store(hash, Ordering::Release);
    FIXTURE_MIDI.fetch_add(
        dummy
            .midi_out
            .iter()
            .map(|buffer| buffer.len() as u32)
            .sum(),
        Ordering::AcqRel,
    );
    for output in &dummy.midi_out {
        for index in 0..output.len() {
            let (_, packet) = output.get(index).unwrap();
            if packet == [0x90, 60, 100] {
                FIXTURE_ON.fetch_add(1, Ordering::AcqRel);
            } else if packet == [0x80, 60, 0] {
                FIXTURE_OFF.fetch_add(1, Ordering::AcqRel);
            } else if packet
                .first()
                .is_some_and(|status| matches!(status & 0xf0, 0x80 | 0x90))
            {
                FIXTURE_BAD_NOTES.fetch_add(1, Ordering::AcqRel);
            }
        }
    }
    result
}

#[cfg(feature = "fixture")]
pub fn fixture_stats() -> [u32; 6] {
    use core::sync::atomic::Ordering;
    [
        FIXTURE_FRAME.load(Ordering::Acquire),
        FIXTURE_HASH.load(Ordering::Acquire),
        FIXTURE_MIDI.load(Ordering::Acquire),
        FIXTURE_ON.load(Ordering::Acquire),
        FIXTURE_OFF.load(Ordering::Acquire),
        FIXTURE_BAD_NOTES.load(Ordering::Acquire),
    ]
}

pub unsafe fn render_step(owner: *mut c_void, frames: usize) -> i32 {
    let dummy = &mut *owner.cast::<Dummy>();
    #[cfg(feature = "usb-midi")]
    let block_time = {
        let now = shoop_usb_micros();
        for (slot, buffer) in dummy.midi_in.iter_mut().enumerate() {
            let mut floor = 0;
            for _ in 0..64 {
                let capacity = crate::MIDI_BLOCK_BYTES.saturating_sub(buffer.byte_len());
                if capacity == 0 {
                    break;
                }
                let mut arrival = 0;
                let len = shoop_usb_read(
                    slot as u32,
                    dummy.usb_scratch.as_mut_ptr(),
                    capacity,
                    &mut arrival,
                );
                if len == 0 {
                    break;
                }
                let offset =
                    shoop_midi_offset(arrival, now, frames as u32, crate::SAMPLE_RATE).max(floor);
                if buffer
                    .push(offset, frames, &dummy.usb_scratch[..len])
                    .is_err()
                {
                    break;
                }
                floor = offset;
            }
        }
        now
    };
    let (left, right) = dummy.output.split_at_mut(1);
    let mut outputs: [&mut [f32]; 2] = [&mut left[0], &mut right[0]];
    let inputs: [&[f32]; 2] = [&dummy.input[0], &dummy.input[1]];
    let result = dummy.render.process(
        AudioBlock {
            frame: dummy.render.next_frame(),
            frames,
            inputs: &inputs,
            outputs: &mut outputs,
        },
        MidiBlock {
            inputs: &dummy.midi_in,
            outputs: &mut dummy.midi_out,
        },
    );
    for input in &mut dummy.midi_in {
        input.clear();
    }
    let Ok(report) = result else {
        #[cfg(feature = "usb-midi")]
        shoop_usb_begin_stop();
        return -1;
    };
    #[cfg(feature = "usb-midi")]
    if report.phase != Phase::Disconnected {
        for (slot, buffer) in dummy.midi_out.iter().enumerate() {
            if dummy
                .render
                .midi_recovery(slot)
                .is_some_and(|recovery| !recovery.is_empty())
            {
                shoop_usb_recover_port(slot as u32);
                continue;
            }
            for index in 0..buffer.len() {
                let Some((offset, bytes)) = buffer.get(index) else {
                    break;
                };
                let due = block_time.saturating_add(
                    (u64::from(offset) * 1_000_000).div_ceil(u64::from(crate::SAMPLE_RATE)),
                );
                shoop_usb_write(slot as u32, bytes.as_ptr(), bytes.len(), due);
            }
        }
    }
    if report.phase == Phase::Disconnected {
        dummy.reset = (0..crate::MIDI_PORTS).all(|port| {
            let mut reset = rt_runtime::midi::ResetState::default();
            reset.request();
            dummy.render.midi_recovery(port).is_some_and(|buffer| {
                for index in 0..buffer.len() {
                    let Some((frame, bytes)) = buffer.get(index) else {
                        return false;
                    };
                    let Some(expected) = reset.peek() else {
                        return false;
                    };
                    if frame != 0 || bytes != expected || reset.acknowledge().is_err() {
                        return false;
                    }
                }
                !reset.pending()
            })
        });
        #[cfg(feature = "usb-midi")]
        shoop_usb_begin_stop();
        return 1;
    }
    0
}

pub unsafe fn render_xruns(owner: *mut c_void, count: u32) {
    (&*owner.cast::<Dummy>()).status.add_xruns(count);
}

struct StoppedDummy {
    render: shoop_rt_engine::Stopped,
    reset: bool,
}
impl StoppedDummy {
    fn cleanup(&mut self) -> bool {
        self.reset && self.render.cleanup().is_ok()
    }
}
unsafe fn retire_render(owner: *mut c_void) -> StoppedDummy {
    let dummy = Box::from_raw(owner.cast::<Dummy>());
    if dummy.reset {
        #[cfg(not(feature = "usb-midi"))]
        log("SHOOP_DUMMY_MIDI_RESET packets=32\n");
        #[cfg(feature = "usb-midi")]
        log("SHOOP_USB_MIDI_RECOVERY_REQUESTED\n");
    }
    dummy.status.set_driver(driver(DriverRunState::Stopped));
    StoppedDummy {
        render: dummy.render.into_stopped(),
        reset: dummy.reset,
    }
}

fn driver(state: DriverRunState) -> DriverStatus {
    DriverStatus {
        kind: DriverKind::Dummy,
        state,
        sample_rate: crate::SAMPLE_RATE,
        buffer_frames: crate::QUANTUM as u32,
    }
}

fn authenticate(
    listener: i32,
    pending: &mut Vec<(Socket, Handshake)>,
    token: &[u8; 32],
    elapsed: Duration,
) -> Vec<Stream<Socket>> {
    for _ in 0..4 {
        let fd = unsafe { shoop_accept(listener) };
        if fd < 0 {
            break;
        }
        if pending.len() < 4 {
            pending.push((
                Socket(fd),
                Handshake::new(*token, true, Duration::from_secs(5)),
            ));
        } else {
            drop(Socket(fd));
        }
    }
    let mut ready = Vec::new();
    let mut index = 0;
    while index < pending.len() {
        let (socket, handshake) = &mut pending[index];
        match handshake.advance(socket, elapsed) {
            Ok(false) => index += 1,
            Ok(true) => {
                let (socket, _) = pending.swap_remove(index);
                if let Ok(stream) = Stream::new(
                    socket,
                    Config {
                        reliable: 8,
                        telemetry: 1,
                        bulk: 8,
                    },
                ) {
                    ready.push(stream);
                }
            }
            Err(_) => {
                pending.swap_remove(index);
            }
        }
    }
    ready
}

pub fn run() {
    let description = EngineDescription {
        quantum: crate::PROCESSING_QUANTUM,
        host: HostPorts::numbered(2, 2, crate::MIDI_PORTS),
        max_frames: crate::QUANTUM,
        midi_bytes: crate::MIDI_BLOCK_BYTES,
        resource_pages: crate::RECORDING_BYTES / 4096,
        segments: vec![Segment {
            strips: 8,
            loops: 32,
        }],
        control_lead_frames: crate::QUANTUM as u32,
        smoothing_limit_frames: 4800,
        ..EngineDescription::default()
    };
    let engine =
        prepare(&description, driver(DriverRunState::Running)).expect("engine preparation");
    let outbox = engine.outbox();
    let dummy = Box::new(Dummy {
        render: engine.render,
        status: engine.host,
        input: [[0.; crate::QUANTUM]; 2],
        output: [[0.; crate::QUANTUM]; 2],
        midi_in: (0..crate::MIDI_PORTS)
            .map(|_| MidiBuffer::new(256, crate::MIDI_BLOCK_BYTES).expect("MIDI input"))
            .collect(),
        midi_out: (0..crate::MIDI_PORTS)
            .map(|_| MidiBuffer::new(256, crate::MIDI_BLOCK_BYTES).expect("MIDI output"))
            .collect(),
        reset: false,
        #[cfg(feature = "usb-midi")]
        usb_scratch: [0; 2048],
    });
    let owner = Box::into_raw(dummy).cast::<c_void>();
    let mut render_owner = Some(owner);
    let mut stopped_owner: Option<StoppedDummy> = None;
    unsafe {
        shoop_start_render(owner);
    }
    let limits = HostResourceLimits {
        media_bytes: crate::MEDIA_BYTES,
        metadata_bytes: 1024 * 1024,
        objects: 512,
        midi_events: 65_536,
    };
    let backend = RtBackend::connect(
        ProtocolClient::new(InProcess::new(engine.endpoint, outbox)),
        Clock,
    )
    .expect("backend connection")
    .with_resource_limits(limits)
    .expect("backend limits");
    #[cfg(feature = "usb-midi")]
    let backend = backend.with_host_port_inventory(Box::new(UsbInventory::new()));
    let mut service = Service::new_with_resource_limits(
        Box::new(backend),
        ServiceOptions {
            master_auto_connect: true,
            loop_smoothing_ms: 3,
        },
        limits,
    )
    .expect("service");
    service
        .set_host_settings(
            HostSettings {
                driver: AudioDriverConfig::Dummy(DummyAudioDriverConfig {
                    sample_rate: crate::SAMPLE_RATE,
                    buffer_size: crate::QUANTUM as u32,
                }),
                pools: NewEnginePools {
                    quantum: crate::PROCESSING_QUANTUM as u32,
                    buses: description.buses as u32,
                    bus_channels: description.bus_channels as u32,
                    tracks: 8,
                    loops: 32,
                    channels: 2,
                    loop_memory_mb: 128,
                },
                master_auto_connect: true,
                loop_smoothing_ms: 3,
            },
            vec![
                String::from("FreeRTOS / AMD lwIP; 1 GiB PS DDR; dummy audio"),
                String::from(
                    "Direct audio/MIDI loops; up to 64 MiB session media and 65,536 MIDI events.",
                ),
            ],
        )
        .expect("settings");
    for _ in 0..10_000 {
        if service.is_ready() {
            break;
        }
        service.tick(Duration::from_millis(1));
        unsafe {
            shoop_delay(1);
        }
    }
    assert!(service.is_ready(), "service initialization");
    let data = Socket(unsafe { shoop_listen(15400) });
    let management = Socket(unsafe { shoop_listen(15401) });
    assert!(data.0 >= 0 && management.0 >= 0, "listeners");
    let mut token = [0; 32];
    let boot = unsafe { shoop_boot_nonce() };
    assert_ne!(
        boot, 0,
        "bootloader must provision a fresh boot nonce at CPU address 0x8000"
    );
    unsafe {
        shoop_token(token.as_mut_ptr());
    }
    let mut pending_data = Vec::with_capacity(4);
    let mut pending_management = Vec::with_capacity(4);
    let mut clients: Vec<Session<Stream<Socket>>> = Vec::with_capacity(4);
    let mut managers: Vec<Management<Stream<Socket>>> = Vec::with_capacity(4);
    let mut last = unsafe { shoop_millis() };
    #[cfg(feature = "usb-midi")]
    let mut last_usb = last;
    let mut shutdown_at = None;
    let mut render_cleaned = false;
    let mut updater_reset = false;
    let mut last_memory = last;
    assert_eq!(
        unsafe { shoop_deployment_ready() },
        1,
        "deployment health confirmation"
    );
    if let Some(info) = deployment_info() {
        service
            .set_deployment_info(info)
            .expect("deployment capability");
    }
    log("SHOOP_HOST_READY\n");
    unsafe {
        shoop_memory_report();
    }
    loop {
        let now = unsafe { shoop_millis() };
        let elapsed = Duration::from_millis(now.saturating_sub(last));
        last = now;
        if now.saturating_sub(last_memory) >= 10_000 {
            unsafe {
                shoop_memory_report();
            }
            last_memory = now;
        }
        for mut port in authenticate(data.0, &mut pending_data, &token, elapsed) {
            if clients.len() >= 4 {
                continue;
            }
            let mut record = [0; MAX_RECORD_BYTES];
            let length = codec::encode(&WireMessage::CarrierReady, &mut record).expect("greeting");
            if port.post(&record[..length]).is_err() {
                continue;
            }
            let mut nonce = unsafe { shoop_nonce() };
            if nonce == boot {
                nonce = unsafe { shoop_nonce() };
            }
            clients.push(
                Session::new(
                    ConnectionId(u128::from(boot) << 64 | u128::from(nonce)),
                    port,
                )
                .with_network(HeartbeatConfig::default(), boot ^ nonce, 30)
                .expect("heartbeat"),
            );
        }
        for port in authenticate(management.0, &mut pending_management, &token, elapsed) {
            if managers.len() < 4 {
                managers.push(Management::new(port));
            }
        }
        for client in &mut clients {
            client.advance(elapsed);
            client.receive(&mut service);
        }
        service.tick(elapsed);
        #[cfg(feature = "usb-midi")]
        if now.saturating_sub(last_usb) >= 100 {
            usb_release_unused(&mut service);
            last_usb = now;
        }
        if service.shutdown_ready() {
            let update = service.updater_request();
            service.begin_host_cleanup().expect("shutdown authority");
            let stopped = if render_owner.is_some() {
                unsafe { shoop_stop_render() }
            } else {
                1
            };
            if stopped == 1 && render_owner.is_some() {
                stopped_owner = Some(unsafe { retire_render(render_owner.take().unwrap()) });
            }
            let cleanup = if stopped != 1 {
                stopped
            } else if render_cleaned {
                1
            } else {
                unsafe { shoop_cleanup_allowed() }
            };
            if cleanup != 2 {
                render_cleaned = render_cleaned
                    || (cleanup == 1 && stopped_owner.as_mut().is_some_and(StoppedDummy::cleanup));
                if render_cleaned {
                    stopped_owner.take();
                }
                let cleaned = render_cleaned
                    && update.is_none_or(|operation| unsafe {
                        shoop_deployment_request(
                            operation.client.to_le_bytes().as_ptr(),
                            operation.sequence,
                        ) == 1
                    });
                service.complete_shutdown(if cleaned {
                    Ok(())
                } else {
                    Err(String::from("render/MIDI cleanup failed"))
                });
                if cleaned {
                    updater_reset = update.is_some();
                    shutdown_at = Some(now);
                    log("SHOOP_HOST_CLEANUP_COMPLETE\n");
                }
            }
        }
        managers.retain_mut(|manager| manager.pump(&mut service, elapsed));
        for client in &mut clients {
            client.write(&mut service);
        }
        clients.retain_mut(|client| {
            let alive = client.alive(&service);
            if !alive {
                service.detach(client.id);
            }
            alive
        });
        if shutdown_at.is_some_and(|time| now.saturating_sub(time) >= 2000) {
            if updater_reset {
                unsafe {
                    shoop_deployment_reset();
                }
            }
            break;
        }
        unsafe {
            shoop_control_progress();
            shoop_delay(2);
        }
    }
    log("SHOOP_HOST_STOPPED\n");
    unsafe {
        shoop_memory_report();
    }
}
