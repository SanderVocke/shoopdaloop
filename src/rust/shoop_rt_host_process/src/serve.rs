use crate::{convert, Port, Transport};
use anyhow::{bail, Result};
use shoop_rt_engine::{ControlEndpoint, EngineDescription, Segment};
use shoop_rt_host_native::{DriverConfig, DummyConfig, NativeConfig, StartedEndpoint};
use shoop_rt_protocol::{
    codec, host::*, ClientMessage, DisconnectReason, EngineMessage, MAX_RECORD_BYTES,
};
use shoop_rt_transport::carrier::{CarrierError, Endpoint, Outbox, RecordPort, RecordServer};
use shoop_rt_transport_shared_memory::{Descriptor, Port as SharedPort, Side, DESCRIPTOR_BYTES};
use shoop_rt_transport_socket::{setup, Config as SocketConfig};
use std::{
    io::{Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
pub fn run() -> Result<()> {
    run_impl(shoop_rt_host_native::start_endpoint)
}
#[cfg(feature = "testing")]
pub fn run_with_start(
    start: impl FnMut(
        &NativeConfig,
    ) -> std::result::Result<StartedEndpoint, shoop_rt_host_native::StartError>,
) -> Result<()> {
    run_impl(start)
}
fn send(control: &mut shoop_rt_transport_socket::Port, reply: HostReply) -> Result<()> {
    let mut bytes = [0; MAX_RECORD_BYTES];
    let len = codec::encode_host_reply(&reply, &mut bytes)
        .map_err(|e| anyhow::anyhow!("host codec: {e:?}"))?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match control.post(&bytes[..len]) {
            Ok(()) => break,
            Err(CarrierError::Full) => {}
            Err(e) => bail!("host connection: {e:?}"),
        }
        if Instant::now() > deadline {
            bail!("host response timed out")
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    while control.pending() != 0 {
        control
            .flush()
            .map_err(|error| anyhow::anyhow!("host flush: {error:?}"))?;
        if Instant::now() > deadline {
            bail!("host response flush timed out")
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}
fn failure(
    control: &mut shoop_rt_transport_socket::Port,
    error: impl std::fmt::Display,
) -> Result<()> {
    send(control, HostReply::Error(Text::clipped(&error.to_string())))
}
struct ProcessEndpoint {
    inner: ControlEndpoint,
    hello: bool,
    retired: bool,
}
impl Endpoint for ProcessEndpoint {
    fn deliver(&mut self, message: ClientMessage, out: &mut Outbox) {
        if self.retired {
            return;
        }
        if matches!(message, ClientMessage::Hello { .. }) {
            if self.hello {
                self.inner.disconnect();
                self.retired = true;
                let _ = out.push(EngineMessage::Disconnected(DisconnectReason::ProtocolError));
                return;
            }
            self.hello = true;
        }
        self.inner.deliver(message, out);
        if matches!(message, ClientMessage::Goodbye) {
            self.retired = true;
        }
    }
    fn poll(&mut self, out: &mut Outbox) {
        self.inner.poll(out);
    }
}
#[cfg(feature = "cpal")]
fn discover(
    request: HostRequest,
    cache: &mut Option<(Text, shoop_rt_host_native::Discovery)>,
) -> Result<HostReply> {
    let HostRequest::Discover { kind, index, host } = request else {
        unreachable!()
    };
    if cache.as_ref().is_none_or(|(old, _)| *old != host) {
        let name = (!host.as_str().is_empty()).then_some(host.as_str());
        *cache = Some((host, shoop_rt_host_native::discover(name)?));
    }
    let discovery = &cache.as_ref().unwrap().1;
    let row = |name: &str,
               default,
               channels,
               sample_rate,
               buffer: Option<(u32, u32)>|
     -> Result<HostReply> {
        Ok(HostReply::Device {
            kind,
            index,
            name: Text::new(name).ok_or_else(|| anyhow::anyhow!("device name exceeds capacity"))?,
            default,
            channels,
            sample_rate,
            min_buffer: buffer.map_or(0, |x| x.0),
            max_buffer: buffer.map_or(0, |x| x.1),
        })
    };
    let i = index as usize;
    match kind {
        DiscoverKind::Hosts => discovery
            .hosts
            .get(i)
            .map(|s| row(s, false, 0, 0, None))
            .unwrap_or(Ok(HostReply::End)),
        DiscoverKind::MidiInputs => discovery
            .midi_inputs
            .get(i)
            .map(|s| row(s, false, 0, 0, None))
            .unwrap_or(Ok(HostReply::End)),
        DiscoverKind::MidiOutputs => discovery
            .midi_outputs
            .get(i)
            .map(|s| row(s, false, 0, 0, None))
            .unwrap_or(Ok(HostReply::End)),
        DiscoverKind::AudioInputs | DiscoverKind::AudioOutputs => {
            let devices = if kind == DiscoverKind::AudioInputs {
                &discovery.inputs
            } else {
                &discovery.outputs
            };
            devices
                .get(i)
                .map(|s| {
                    row(
                        &s.name,
                        s.default,
                        s.channels,
                        s.sample_rate,
                        s.buffer_frames,
                    )
                })
                .unwrap_or(Ok(HostReply::End))
        }
    }
}
fn run_impl(
    mut start: impl FnMut(
        &NativeConfig,
    ) -> std::result::Result<StartedEndpoint, shoop_rt_host_native::StartError>,
) -> Result<()> {
    let address = std::env::var("SHOOP_RT_PROCESS_ADDRESS")?.parse()?;
    let transport = Transport::parse(&std::env::var("SHOOP_RT_PROCESS_TRANSPORT")?)?;
    let mut input = std::io::stdin().lock();
    let mut control = setup::connect(address, SocketConfig::default(), Duration::from_secs(10))?;
    let mut hello = false;
    let mut engine: Option<EngineDescription> = None;
    let mut driver = None;
    let mut jack = convert::JackSetup::default();
    let mut midi_inputs = Vec::new();
    let mut midi_outputs = Vec::new();
    #[cfg(feature = "cpal")]
    let mut cache = None;
    let (started, data) = loop {
        let mut bytes = [0; MAX_RECORD_BYTES];
        let Some(len) = control.take(&mut bytes) else {
            if control.failure().is_some() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(1));
            continue;
        };
        let request = match codec::decode_host_request(&bytes[..len]) {
            Ok(request) => request,
            Err(error) => {
                failure(&mut control, format!("invalid host setup: {error:?}"))?;
                return Ok(());
            }
        };
        if !hello
            && !matches!(
                request,
                HostRequest::Hello {
                    version: HOST_VERSION
                }
            )
        {
            failure(&mut control, "unsupported initial host handshake")?;
            return Ok(());
        }
        let result: Result<HostReply> = (|| {
            Ok(match request {
                HostRequest::Hello { version } if !hello && version == HOST_VERSION => {
                    hello = true;
                    HostReply::Hello {
                        version: HOST_VERSION,
                    }
                }
                HostRequest::Hello { .. } => {
                    bail!("host connection cannot be restarted")
                }
                HostRequest::ConfigureEngine(settings) => {
                    if engine.is_some() {
                        bail!("engine settings already supplied")
                    }
                    engine = Some(convert::native_engine(settings)?);
                    HostReply::Ok
                }
                HostRequest::AddSegment { strips, loops } => {
                    let engine = engine
                        .as_mut()
                        .ok_or_else(|| anyhow::anyhow!("engine settings missing"))?;
                    if engine.segments.len() >= MAX_SETUP_ITEMS {
                        bail!("too many segments")
                    }
                    engine.segments.push(Segment { strips, loops });
                    HostReply::Ok
                }
                HostRequest::ConfigureDriver(settings) => {
                    if driver.is_some() {
                        bail!("driver settings already supplied")
                    }
                    driver = Some(settings);
                    HostReply::Ok
                }
                HostRequest::AddJackAudioInput(name) => {
                    if jack.inputs.len() >= 64 {
                        bail!("too many JACK audio inputs");
                    }
                    jack.inputs.push(name.as_str().into());
                    HostReply::Ok
                }
                HostRequest::AddJackAudioOutput(name) => {
                    if jack.outputs.len() >= 64 {
                        bail!("too many JACK audio outputs");
                    }
                    jack.outputs.push(name.as_str().into());
                    HostReply::Ok
                }
                HostRequest::ConnectJack {
                    source,
                    destination,
                } => {
                    if jack.connections.len() >= MAX_SETUP_ITEMS {
                        bail!("too many JACK connections");
                    }
                    jack.connections
                        .push((source.as_str().into(), destination.as_str().into()));
                    HostReply::Ok
                }
                HostRequest::AddMidiInput(name) => {
                    if midi_inputs.len() >= MAX_SETUP_ITEMS {
                        bail!("too many MIDI inputs")
                    }
                    midi_inputs.push(name.as_str().into());
                    HostReply::Ok
                }
                HostRequest::AddMidiOutput(name) => {
                    if midi_outputs.len() >= MAX_SETUP_ITEMS {
                        bail!("too many MIDI outputs")
                    }
                    midi_outputs.push(name.as_str().into());
                    HostReply::Ok
                }
                HostRequest::Discover { .. } => {
                    #[cfg(feature = "cpal")]
                    {
                        discover(request, &mut cache)?
                    }
                    #[cfg(not(feature = "cpal"))]
                    {
                        bail!("device discovery requires CPAL")
                    }
                }
                HostRequest::Status => HostReply::Status {
                    frame: 0,
                    callbacks: 0,
                },
                HostRequest::Stop => return Ok(HostReply::Stopped { midi_reset: true }),
                HostRequest::Start => HostReply::Ok,
            })
        })();
        if matches!(request, HostRequest::Stop) {
            send(&mut control, HostReply::Stopped { midi_reset: true })?;
            return Ok(());
        }
        if !matches!(request, HostRequest::Start) {
            match result {
                Ok(reply) => send(&mut control, reply)?,
                Err(error) => failure(&mut control, error)?,
            }
            continue;
        }
        let Some(engine) = engine.take() else {
            failure(&mut control, "engine settings missing")?;
            return Ok(());
        };
        let Some(settings) = driver.take() else {
            failure(&mut control, "driver settings missing")?;
            return Ok(());
        };
        let native_driver = match convert::native_driver(
            settings,
            std::mem::take(&mut midi_inputs),
            std::mem::take(&mut midi_outputs),
            std::mem::take(&mut jack),
        ) {
            Ok(driver) => driver,
            Err(error) => {
                failure(&mut control, error)?;
                return Ok(());
            }
        };
        let shared = if transport == Transport::SharedMemory {
            let mut descriptor = [0; DESCRIPTOR_BYTES];
            input.read_exact(&mut descriptor)?;
            Some(SharedPort::open(
                Descriptor::decode(&descriptor)?,
                Side::Server,
            )?)
        } else {
            None
        };
        let mut config = NativeConfig {
            engine,
            driver: native_driver,
        };
        let started = match start(&config) {
            Ok(started) => started,
            Err(error) if !matches!(config.driver, DriverConfig::Dummy(_)) => {
                send(
                    &mut control,
                    HostReply::Warning(Text::clipped(&format!(
                        "Could not start the configured audio driver: {error}; using the dummy driver"
                    ))),
                )?;
                let dummy = DummyConfig::default();
                config.engine.max_frames = dummy.buffer_frames as usize;
                config.driver = DriverConfig::Dummy(dummy);
                match start(&config) {
                    Ok(started) => started,
                    Err(error) => {
                        failure(&mut control, error)?;
                        return Ok(());
                    }
                }
            }
            Err(error) => {
                failure(&mut control, error)?;
                return Ok(());
            }
        };
        for warning in started.host.warnings() {
            send(&mut control, HostReply::Warning(Text::clipped(warning)))?;
        }
        send(
            &mut control,
            HostReply::InstanceName(Text::clipped(started.host.instance_name())),
        )?;
        send(&mut control, HostReply::Ready(started.host.status().driver))?;
        let data = if let Some(shared) = shared {
            let liveness = control.stream().try_clone()?;
            drop(control);
            Port::shared(shared.with_liveness(liveness)?)
        } else {
            Port::socket(control)
        };
        break (started, data);
    };
    drop(input);
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let disconnect = Arc::new(AtomicBool::new(false));
    let disconnecting = disconnect.clone();
    #[cfg(feature = "testing")]
    let (step_tx, step_rx) = std::sync::mpsc::sync_channel::<(u64, u64)>(16);
    let _watcher = std::thread::spawn(move || {
        let mut input = std::io::stdin().lock();
        loop {
            let mut tag = [0];
            if input.read_exact(&mut tag).is_err() {
                disconnecting.store(true, Ordering::Release);
                return;
            }
            match tag[0] {
                b'Q' => {
                    stopping.store(true, Ordering::Release);
                    return;
                }
                #[cfg(feature = "testing")]
                b'D' => {
                    disconnecting.store(true, Ordering::Release);
                }
                #[cfg(feature = "testing")]
                b'A' => {
                    let mut numbers = [0; 16];
                    if input.read_exact(&mut numbers).is_err() {
                        return;
                    }
                    let frames = u64::from_le_bytes(numbers[..8].try_into().unwrap());
                    let ingress = u64::from_le_bytes(numbers[8..].try_into().unwrap());
                    if step_tx.send((frames, ingress)).is_err() {
                        return;
                    }
                }
                _ => {
                    disconnecting.store(true, Ordering::Release);
                    return;
                }
            }
        }
    });
    let StartedEndpoint {
        host,
        endpoint,
        outbox,
    } = started;
    #[cfg(feature = "testing")]
    let received = data.received.clone();
    #[cfg(feature = "testing")]
    let sent = data.sent.clone();
    #[cfg(feature = "testing")]
    let mut pending_step = None;
    let mut connection = Some(RecordServer::new(
        ProcessEndpoint {
            inner: endpoint,
            hello: false,
            retired: false,
        },
        data,
        outbox,
    ));
    let mut retired: Option<ControlEndpoint> = None;
    let mut retired_outbox = Outbox::new(8, 0, 0);
    #[cfg(feature = "testing")]
    let status_file = std::env::var_os("SHOOP_RT_TEST_STATUS_FILE");
    #[cfg(feature = "testing")]
    let mut last_status = Instant::now();
    while !stop.load(Ordering::Acquire) {
        if let Some(server) = connection.as_mut() {
            server.pump(32);
            let channel_lost = disconnect.load(Ordering::Acquire);
            let driver_failed =
                host.status().driver.state == shoop_rt_protocol::DriverRunState::Failed;
            if server.failure().is_some()
                || server.endpoint().retired
                || channel_lost
                || driver_failed
            {
                tracing::info!(failure=?server.failure(), channel_lost, driver_failed, "engine_process.control_retired");
                let (mut endpoint, _port, _outbox) = connection.take().unwrap().into_parts();
                endpoint.inner.disconnect();
                retired = Some(endpoint.inner);
            }
        }
        if let Some(endpoint) = retired.as_mut() {
            endpoint.poll(&mut retired_outbox);
            while retired_outbox.pop().is_some() {}
        }
        #[cfg(feature = "testing")]
        {
            if pending_step.is_none() {
                pending_step = step_rx.try_recv().ok();
            }
            if let Some((frames, ingress)) = pending_step {
                if received.load(Ordering::Acquire) >= ingress {
                    if let Some(dummy) = host.dummy().filter(|_| frames != 0) {
                        dummy.request_frames(frames);
                        if !dummy.wait_rendered(Duration::from_secs(5)) {
                            bail!("test driver did not advance")
                        }
                    }
                    if let Some(server) = connection.as_mut() {
                        server.pump(0);
                    }
                    let _ = writeln!(
                        std::io::stdout(),
                        "@FRAME {} {}",
                        host.status().next_frame,
                        sent.load(Ordering::Acquire)
                    );
                    pending_step = None;
                }
            }
        }
        #[cfg(feature = "testing")]
        if last_status.elapsed() >= Duration::from_millis(20) {
            if let Some(path) = &status_file {
                let status = host.status();
                let _ = std::fs::write(path, format!("{:?} {}", status.phase, status.next_frame));
            }
            last_status = Instant::now();
        }
        std::thread::sleep(Duration::from_micros(200));
    }
    drop(connection);
    drop(retired);
    let report = host.stop();
    let flags = u8::from(report.midi_reset) | if report.stopped.is_ok() { 2 } else { 0 };
    let _ = writeln!(std::io::stdout(), "@STOP {flags}");
    if report.stopped.is_err() {
        bail!("engine stopped cleanup failed")
    }
    Ok(())
}
