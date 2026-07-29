//! Cross-platform driver built on `cpal`, for hosts without JACK.
//!
//! Behind the `cpal` feature. `cpal` rather than miniaudio, which the design notes
//! originally preferred for its true duplex: the `miniaudio` Rust binding is pinned to
//! bindgen 0.54 and no longer builds at all, whereas `cpal` is pure Rust and works.
//!
//! Output alone via [`start_output`], or capture as well via [`start_duplex`].
//!
//! Duplex is the awkward part, and the reason the design originally wanted true duplex.
//! `cpal` gives independent input and output streams with their own callbacks and no
//! shared clock, so capture cannot be handed to the same cycle the way JACK's single
//! callback allows. What bridges them here is a ring: the input callback pushes its
//! samples in, and the output callback -- which is what drives the engine -- takes a
//! cycle's worth out.
//!
//! That leaves drift, which is handled by refusing to hide it. A cycle that finds less
//! than it needs gets silence for the remainder and counts a
//! `capture_underruns`; a ring that fills drops the oldest samples and counts
//! `capture_overruns`. Both are in [`crate::engine::Stats`], so persistent drift shows
//! up as a number rather than as mysterious glitches. Latency is one ring's worth,
//! deliberately small.
//!
//! Interleaving is the other difference from JACK. `cpal` hands over one interleaved
//! buffer for all channels, where JACK gives a separate buffer per port, so this
//! de-interleaves into the session's ports and interleaves the result back.
//!
//! The driver is generic over a [`HostTrait`] so the same code path drives a real OS
//! audio backend in production and a software host in tests -- the latter lets the
//! headless CI run the audio-thread tests without an ALSA device or a PulseAudio
//! server, which on a fresh Debian image is exactly the case that fails today.

use crate::driver::Driver;
use crate::engine::{EngineHandle, Stats};
use crate::external_audio_port::ExternalAudioPort;
use crate::port::PortDirection;
use crate::session::{Port, Session};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::RingBuffer;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CpalError {
    #[error("no default output device")]
    NoOutputDevice,
    #[error("no default input device")]
    NoInputDevice,
    #[error("could not read the device's default configuration: {0}")]
    Config(#[from] cpal::DefaultStreamConfigError),
    #[error("could not build the stream: {0}")]
    Build(#[from] cpal::BuildStreamError),
    #[error("could not start the stream: {0}")]
    Play(#[from] cpal::PlayStreamError),
    #[error("device reports {0} channels, which is more than were registered")]
    TooManyChannels(u16),
}

/// A running stream pair with the engine on the output callback's thread.
///
/// The streams are held as boxed trait objects so the same driver works for any
/// [`HostTrait`] implementation: the platform streams on a real machine, a software
/// host in tests. Dropping the boxes stops the audio threads.
pub struct CpalDriver {
    _output: Box<dyn StreamTrait + Send>,
    _input: Option<Box<dyn StreamTrait + Send>>,
    handle: EngineHandle,
    sample_rate: u32,
    /// Frames in the last cycle. `cpal` does not commit to a buffer size, so this is
    /// published by the callback rather than known at start.
    buffer_size: Arc<std::sync::atomic::AtomicU32>,
    client_name: String,
    stats: Arc<Stats>,
    n_channels: u16,
    n_capture_channels: u16,
}

impl Driver for CpalDriver {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    /// The device's buffer size is not fixed, so this is the last cycle's frame count,
    /// and zero until one has run.
    fn buffer_size(&self) -> u32 {
        self.buffer_size.load(Ordering::Relaxed)
    }
    fn client_name(&self) -> &str {
        &self.client_name
    }
    fn stats(&self) -> &Arc<Stats> {
        &self.stats
    }
    fn handle(&mut self) -> &mut EngineHandle {
        &mut self.handle
    }
}

/// Starts an output stream on the OS default host, registering one session port per
/// device channel.
///
/// `setup` runs before the engine is handed over, with the port indices that were
/// registered, so a caller can wire its graph to them.
pub fn start_output<F>(
    session: Session,
    command_queue_capacity: usize,
    setup: F,
) -> Result<CpalDriver, CpalError>
where
    F: FnOnce(&mut Session, &[usize]) -> Result<(), CpalError> + Send + 'static,
{
    start_output_on_host(cpal::default_host(), session, command_queue_capacity, setup)
}

/// Starts an output stream on a caller-supplied host.
///
/// Tests use this with a software host that fires its callbacks from a regular thread,
/// so the audio path is exercised without a real ALSA/CoreAudio/WASAPI device.
pub fn start_output_on_host<H, F>(
    host: H,
    session: Session,
    command_queue_capacity: usize,
    setup: F,
) -> Result<CpalDriver, CpalError>
where
    H: HostTrait,
    H::Device: 'static,
    <H::Device as DeviceTrait>::Stream: Send + 'static,
    F: FnOnce(&mut Session, &[usize]) -> Result<(), CpalError>,
{
    // No hook: the common case, where nothing feeds the engine from inside the callback.
    let (driver, ()) =
        start_output_with_hook_on_host(host, session, command_queue_capacity, |s, ports| {
            setup(s, ports)?;
            Ok(((), Box::new(|_: &mut Session, _: usize| {}) as CycleHook))
        })?;
    Ok(driver)
}

/// As [`start_output`], but `setup` also builds something that runs each cycle before the
/// engine does.
///
/// For a source that lives in the callback rather than behind a device -- a built-in
/// instrument, say. `setup` returns a value for the caller to keep and a hook for the
/// audio thread, which is the only arrangement that works: the hook needs port indices
/// that only exist once setup has run, and the caller needs the rest of what setup built.
///
/// The hook stages into the session's ports, so what it produces is indistinguishable
/// from something a device sent. It runs on the audio thread and must not allocate.
pub type CycleHook = Box<dyn FnMut(&mut Session, usize) + Send>;

pub fn start_output_with_hook<F, T>(
    session: Session,
    command_queue_capacity: usize,
    setup: F,
) -> Result<(CpalDriver, T), CpalError>
where
    F: FnOnce(&mut Session, &[usize]) -> Result<(T, CycleHook), CpalError> + Send + 'static,
{
    start_output_with_hook_on_host(cpal::default_host(), session, command_queue_capacity, setup)
}

/// As [`start_output_with_hook`], but on a caller-supplied host.
pub fn start_output_with_hook_on_host<H, F, T>(
    host: H,
    session: Session,
    command_queue_capacity: usize,
    setup: F,
) -> Result<(CpalDriver, T), CpalError>
where
    H: HostTrait,
    H::Device: 'static,
    <H::Device as DeviceTrait>::Stream: Send + 'static,
    F: FnOnce(&mut Session, &[usize]) -> Result<(T, CycleHook), CpalError>,
{
    start_output_on_device(host, session, command_queue_capacity, None, setup)
}

/// As [`start_output_with_hook_on_host`], but on a named device.
///
/// An unknown name falls back to the default, so a stored choice for hardware that has been
/// unplugged does not stop the application from starting.
pub fn start_output_on_device<H, F, T>(
    host: H,
    session: Session,
    command_queue_capacity: usize,
    preferred_device: Option<String>,
    setup: F,
) -> Result<(CpalDriver, T), CpalError>
where
    H: HostTrait,
    H::Device: 'static,
    <H::Device as DeviceTrait>::Stream: Send + 'static,
    F: FnOnce(&mut Session, &[usize]) -> Result<(T, CycleHook), CpalError>,
{
    let device = match preferred_device.as_deref() {
        // Chosen by name, falling back to the default rather than failing: a stored choice
        // for a device that has since been unplugged should not stop the application.
        Some(wanted) => host
            .output_devices()
            .ok()
            .and_then(|mut ds| ds.find(|d| device_label(d) == wanted))
            .or_else(|| host.default_output_device()),
        None => host.default_output_device(),
    }
    .ok_or(CpalError::NoOutputDevice)?;
    let config = device.default_output_config()?;
    let sample_rate = config.sample_rate().0;
    let n_channels = config.channels();

    let mut session = session;
    let port_indices: Vec<usize> = (0..n_channels)
        .map(|c| {
            session.add_port(Port::External(ExternalAudioPort::new(
                format!("out_{c}"),
                PortDirection::Output,
                // No capture ring on an output port: nothing records from it.
                0,
            )))
        })
        .collect();

    session.set_sample_rate(sample_rate);
    let (built, mut hook) = setup(&mut session, &port_indices)?;
    let _ = session.apply_graph_changes();

    let (mut engine, handle) = crate::engine::split(session, command_queue_capacity);
    let stats = Arc::clone(handle.stats());
    let n = n_channels as usize;
    let device_name = device_label(&device);
    let cb_stats = Arc::clone(&stats);
    let buffer_size = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let cb_buffer_size = Arc::clone(&buffer_size);

    let stream = device.build_output_stream(
        &config.into(),
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            // `data` is interleaved across all channels, so the frame count is what the
            // engine is asked for, not the buffer length.
            let began = std::time::Instant::now();
            let n_frames = data.len().checked_div(n).unwrap_or(0);
            cb_buffer_size.store(n_frames as u32, Ordering::Relaxed);
            // Before the cycle, so anything it stages is picked up by the ports' prepare.
            hook(engine.session_mut(), n_frames);
            engine.process(n_frames);

            for (c, &pi) in port_indices.iter().enumerate() {
                let produced = engine
                    .session()
                    .port(pi)
                    .and_then(Port::as_external)
                    .map(|p| p.output(n_frames));
                match produced {
                    Some(src) => {
                        for f in 0..n_frames {
                            // Silence past what the port produced, rather than leaving
                            // whatever the device buffer happened to contain.
                            data[f * n + c] = src.get(f).copied().unwrap_or(0.0);
                        }
                    }
                    None => {
                        for f in 0..n_frames {
                            data[f * n + c] = 0.0;
                        }
                    }
                }
            }

            // Measured rather than asked for: cpal exposes no load figure, so the callback's
            // own duration against the buffer's is the honest estimate. A clock read is cheap
            // and is what an audio host normally does for this.
            if n_frames > 0 && sample_rate > 0 {
                let budget = n_frames as f64 / sample_rate as f64;
                let spent = began.elapsed().as_secs_f64();
                cb_stats.set_dsp_load_percent(((spent / budget) * 100.0) as f32);
            }
        },
        |err| {
            // Nowhere useful to report from a device callback, and panicking here would
            // cross a foreign frame. Xruns are visible through the engine's stats.
            let _ = err;
        },
        None,
    )?;
    stream.play()?;

    Ok((
        CpalDriver {
            _output: Box::new(stream),
            _input: None,
            handle,
            sample_rate,
            buffer_size,
            client_name: device_name,
            stats,
            n_channels,
            n_capture_channels: 0,
        },
        built,
    ))
}

/// Output devices the host offers, by name.
///
/// Names rather than handles, because a device chosen in a UI has to survive being stored
/// and re-resolved; a handle would not.
pub fn output_device_names() -> Vec<String> {
    let host = cpal::default_host();
    match host.output_devices() {
        Ok(devices) => devices.map(|d| device_label(&d)).collect(),
        Err(_) => Vec::new(),
    }
}

/// The default output device's name, if there is one.
pub fn default_output_device_name() -> Option<String> {
    cpal::default_host()
        .default_output_device()
        .map(|d| device_label(&d))
}

/// A device's name, or a placeholder when the backend will not give one.
fn device_label<D: DeviceTrait>(device: &D) -> String {
    device.name().unwrap_or_else(|_| "cpal".to_string())
}

impl CpalDriver {
    /// Queues control work. The only way to reach the engine once running.
    pub fn handle(&mut self) -> &mut EngineHandle {
        &mut self.handle
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    pub fn n_channels(&self) -> u16 {
        self.n_channels
    }
    /// Zero unless started with [`start_duplex`].
    pub fn n_capture_channels(&self) -> u16 {
        self.n_capture_channels
    }
}

/// Starts an output stream and an input stream, bridged by a ring.
///
/// `setup` is given the output port indices and then the input port indices, so a
/// caller can wire either side.
///
/// `ring_frames` bounds the capture latency and how much drift can be absorbed before a
/// cycle underruns. A few buffers' worth is the useful range: too small and every cycle
/// underruns, too large and capture lags audibly behind playback.
pub fn start_duplex<F>(
    session: Session,
    command_queue_capacity: usize,
    ring_frames: usize,
    setup: F,
) -> Result<CpalDriver, CpalError>
where
    F: FnOnce(&mut Session, &[usize], &[usize]) -> Result<(), CpalError> + Send + 'static,
{
    start_duplex_on_host(
        cpal::default_host(),
        session,
        command_queue_capacity,
        ring_frames,
        setup,
    )
}

/// As [`start_duplex`], but on a caller-supplied host.
pub fn start_duplex_on_host<H, F>(
    host: H,
    session: Session,
    command_queue_capacity: usize,
    ring_frames: usize,
    setup: F,
) -> Result<CpalDriver, CpalError>
where
    H: HostTrait,
    H::Device: 'static,
    <H::Device as DeviceTrait>::Stream: Send + 'static,
    F: FnOnce(&mut Session, &[usize], &[usize]) -> Result<(), CpalError>,
{
    let out_device = host
        .default_output_device()
        .ok_or(CpalError::NoOutputDevice)?;
    let in_device = host
        .default_input_device()
        .ok_or(CpalError::NoInputDevice)?;

    let out_config = out_device.default_output_config()?;
    let in_config = in_device.default_input_config()?;
    let sample_rate = out_config.sample_rate().0;
    let n_out = out_config.channels();
    let n_in = in_config.channels();

    let mut session = session;
    let out_ports: Vec<usize> = (0..n_out)
        .map(|c| {
            session.add_port(Port::External(ExternalAudioPort::new(
                format!("out_{c}"),
                PortDirection::Output,
                0,
            )))
        })
        .collect();
    let in_ports: Vec<usize> = (0..n_in)
        .map(|c| {
            session.add_port(Port::External(ExternalAudioPort::new(
                format!("in_{c}"),
                PortDirection::Input,
                // A capture ring, so an input port can be recorded retroactively.
                sample_rate as usize,
            )))
        })
        .collect();

    session.set_sample_rate(sample_rate);
    setup(&mut session, &out_ports, &in_ports)?;
    let _ = session.apply_graph_changes();

    let (mut engine, handle) = crate::engine::split(session, command_queue_capacity);
    let stats = Arc::clone(handle.stats());
    let driver_stats = Arc::clone(&stats);
    let out_name = device_label(&out_device);
    let buffer_size = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let cb_buffer_size = Arc::clone(&buffer_size);

    // Interleaved samples, so the ring is sized in samples rather than frames.
    let (mut ring_tx, mut ring_rx) = RingBuffer::<f32>::new(ring_frames * n_in as usize);

    let in_stats = Arc::clone(&stats);
    let input = in_device.build_input_stream(
        &in_config.into(),
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let mut dropped = 0u32;
            for &s in data {
                if ring_tx.push(s).is_err() {
                    // Full: the output side is not draining fast enough. Counted rather
                    // than blocked on, which would stall a device callback.
                    dropped += 1;
                }
            }
            if dropped > 0 {
                in_stats
                    .capture_overruns
                    .fetch_add(dropped, Ordering::Relaxed);
            }
        },
        |err| {
            let _ = err;
        },
        None,
    )?;

    let out_stats = Arc::clone(&stats);
    let n = n_out as usize;
    let nin = n_in as usize;
    // Reused across cycles so the callback does not allocate.
    let mut scratch: Vec<f32> = vec![0.0; ring_frames.max(1) * nin.max(1)];

    let output = out_device.build_output_stream(
        &out_config.into(),
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            let n_frames = data.len().checked_div(n).unwrap_or(0);
            cb_buffer_size.store(n_frames as u32, Ordering::Relaxed);

            // Take a cycle's worth of capture, padding with silence if the input stream
            // has not produced enough. Underruns are counted, not hidden.
            let wanted = n_frames * nin;
            if scratch.len() < wanted {
                // Only on a buffer-size increase, and off the steady state.
                scratch.resize(wanted, 0.0);
            }
            let mut got = 0;
            while got < wanted {
                match ring_rx.pop() {
                    Ok(s) => {
                        scratch[got] = s;
                        got += 1;
                    }
                    Err(_) => break,
                }
            }
            if got < wanted {
                for s in &mut scratch[got..wanted] {
                    *s = 0.0;
                }
                out_stats.capture_underruns.fetch_add(1, Ordering::Relaxed);
            }

            for (c, &pi) in in_ports.iter().enumerate() {
                if let Some(p) = engine
                    .session_mut()
                    .port_mut(pi)
                    .and_then(Port::as_external_mut)
                {
                    // De-interleaved straight into the port, so no per-channel plane has
                    // to be built on this thread.
                    p.stage_input_strided(&scratch[..wanted], c, nin);
                }
            }

            engine.process(n_frames);

            for (c, &pi) in out_ports.iter().enumerate() {
                let produced = engine
                    .session()
                    .port(pi)
                    .and_then(Port::as_external)
                    .map(|p| p.output(n_frames));
                match produced {
                    Some(src) => {
                        for f in 0..n_frames {
                            data[f * n + c] = src.get(f).copied().unwrap_or(0.0);
                        }
                    }
                    None => {
                        for f in 0..n_frames {
                            data[f * n + c] = 0.0;
                        }
                    }
                }
            }
        },
        |err| {
            let _ = err;
        },
        None,
    )?;

    input.play()?;
    output.play()?;

    Ok(CpalDriver {
        _output: Box::new(output),
        _input: Some(Box::new(input)),
        handle,
        sample_rate,
        buffer_size,
        client_name: out_name,
        stats: driver_stats,
        n_channels: n_out,
        n_capture_channels: n_in,
    })
}
