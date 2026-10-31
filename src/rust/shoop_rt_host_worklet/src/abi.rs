//! The module's exports, called by the processor script on the audio thread.
//! Each module instance holds one host in a module-global cell. Pointers are
//! byte offsets into the instance's memory; nothing here dereferences them.
#![allow(unsafe_code)] // `no_mangle` only.

use std::cell::RefCell;

use crate::config::{WorkletConfig, MAX_CONFIG_BYTES};
use crate::host::{Inbound, Outbound, PrepareFailure, Quantum, QuantumError, WorkletHost};

pub const PREPARED: u32 = 0;
pub const ALREADY_PREPARED: u32 = 1;
pub const MISSING_CONFIG: u32 = 2;
pub const INVALID_CONFIG: u32 = 3;
pub const ENGINE_REFUSED: u32 = 4;

pub const RENDERED: u32 = 0;
pub const NOT_PREPARED: u32 = 1;
pub const INVALID_QUANTUM: u32 = 2;
pub const FAULTED: u32 = 3;

pub const RECORDS_IN: u32 = 0;
pub const RECORDS_OUT: u32 = 1;
pub const MIDI_IN: u32 = 2;
pub const MIDI_OUT: u32 = 3;

pub const DISCONTINUITIES: u32 = 0;
pub const OVERRUNS: u32 = 1;
pub const MIDI_IN_DROPPED: u32 = 2;
pub const MIDI_IN_LATE: u32 = 3;
pub const MIDI_OUT_DROPPED: u32 = 4;
pub const FRAMING_ERRORS: u32 = 5;

struct Worklet {
    config: Option<Box<[u8]>>,
    host: Option<WorkletHost>,
}

thread_local! {
    static WORKLET: RefCell<Worklet> = const {
        RefCell::new(Worklet {
            config: None,
            host: None,
        })
    };
}

fn with_host<R>(unprepared: R, f: impl FnOnce(&mut WorkletHost) -> R) -> R {
    WORKLET.with(|worklet| worklet.borrow_mut().host.as_mut().map_or(unprepared, f))
}

enum Buffer {
    In(Inbound),
    Out(Outbound),
}

fn buffer(id: u32) -> Option<Buffer> {
    Some(match id {
        RECORDS_IN => Buffer::In(Inbound::Records),
        RECORDS_OUT => Buffer::Out(Outbound::Records),
        MIDI_IN => Buffer::In(Inbound::Midi),
        MIDI_OUT => Buffer::Out(Outbound::Midi),
        _ => return None,
    })
}

/// Space for a configuration record of `len` bytes, before preparation.
#[no_mangle]
pub extern "C" fn shoop_rt_worklet_config_buffer(len: usize) -> *mut u8 {
    WORKLET.with(|worklet| {
        let mut worklet = worklet.borrow_mut();
        if len == 0 || len > MAX_CONFIG_BYTES || worklet.host.is_some() {
            return std::ptr::null_mut();
        }
        worklet
            .config
            .insert(vec![0; len].into_boxed_slice())
            .as_mut_ptr()
    })
}

/// Prepare the engine from the configuration record.
#[no_mangle]
pub extern "C" fn shoop_rt_worklet_prepare() -> u32 {
    WORKLET.with(|worklet| {
        let mut worklet = worklet.borrow_mut();
        if worklet.host.is_some() {
            return ALREADY_PREPARED;
        }
        let Some(record) = worklet.config.take() else {
            return MISSING_CONFIG;
        };
        let Ok(config) = WorkletConfig::decode(&record) else {
            return INVALID_CONFIG;
        };
        match WorkletHost::prepare(&config) {
            Ok(host) => {
                worklet.host = Some(host);
                PREPARED
            }
            Err(PrepareFailure::Config(_)) => INVALID_CONFIG,
            Err(PrepareFailure::Engine(_)) => ENGINE_REFUSED,
        }
    })
}

/// Drop the host and its memory. The module can be prepared again.
#[no_mangle]
pub extern "C" fn shoop_rt_worklet_release() {
    WORKLET.with(|worklet| {
        let mut worklet = worklet.borrow_mut();
        worklet.host = None;
        worklet.config = None;
    });
}

#[no_mangle]
pub extern "C" fn shoop_rt_worklet_max_frames() -> u32 {
    with_host(0, |host| host.max_frames() as u32)
}

#[no_mangle]
pub extern "C" fn shoop_rt_worklet_audio_inputs() -> u32 {
    with_host(0, |host| host.audio_inputs() as u32)
}

#[no_mangle]
pub extern "C" fn shoop_rt_worklet_audio_outputs() -> u32 {
    with_host(0, |host| host.audio_outputs() as u32)
}

/// Planar host audio inputs, `max_frames` samples per channel.
#[no_mangle]
pub extern "C" fn shoop_rt_worklet_audio_input_ptr() -> *mut f32 {
    with_host(std::ptr::null_mut(), WorkletHost::input_ptr)
}

/// Planar host audio outputs, `max_frames` samples per channel.
#[no_mangle]
pub extern "C" fn shoop_rt_worklet_audio_output_ptr() -> *const f32 {
    with_host(std::ptr::null(), |host| host.output_ptr())
}

#[no_mangle]
pub extern "C" fn shoop_rt_worklet_buffer_ptr(id: u32) -> *mut u8 {
    with_host(std::ptr::null_mut(), |host| match buffer(id) {
        Some(Buffer::In(lane)) => host.with_inbound(lane, |stream| stream.as_mut_ptr()),
        Some(Buffer::Out(lane)) => host.outbound_ptr(lane).0.cast_mut(),
        None => std::ptr::null_mut(),
    })
}

#[no_mangle]
pub extern "C" fn shoop_rt_worklet_buffer_capacity(id: u32) -> usize {
    with_host(0, |host| match buffer(id) {
        Some(Buffer::In(lane)) => host.with_inbound(lane, |stream| stream.capacity()),
        Some(Buffer::Out(lane)) => host.outbound_ptr(lane).1,
        None => 0,
    })
}

/// Inbound: where the next posted bytes go. Outbound: the bytes of the last
/// quantum.
#[no_mangle]
pub extern "C" fn shoop_rt_worklet_buffer_len(id: u32) -> usize {
    with_host(0, |host| match buffer(id) {
        Some(Buffer::In(lane)) => host.with_inbound(lane, |stream| stream.len()),
        Some(Buffer::Out(lane)) => host.read(lane, <[u8]>::len),
        None => 0,
    })
}

/// Accept `len` bytes written at an inbound buffer's length.
#[no_mangle]
pub extern "C" fn shoop_rt_worklet_commit(id: u32, len: usize) -> bool {
    with_host(false, |host| match buffer(id) {
        Some(Buffer::In(lane)) => host.with_inbound(lane, |stream| stream.commit(len)),
        _ => false,
    })
}

/// Render one quantum of `frames` frames starting at `currentFrame` `frame`.
/// `load` is the previous quantum's processing time as a share of its period,
/// or negative when unknown.
#[no_mangle]
pub extern "C" fn shoop_rt_worklet_process(
    frames: u32,
    input_channels: u32,
    frame: f64,
    load: f64,
) -> u32 {
    with_host(NOT_PREPARED, |host| {
        let quantum = Quantum {
            frames: frames as usize,
            input_channels: input_channels as usize,
            frame: frame as u64,
            load: (load >= 0.).then_some(load as f32),
        };
        match host.process(quantum) {
            Ok(()) => RENDERED,
            Err(QuantumError::Invalid) => INVALID_QUANTUM,
            Err(QuantumError::Faulted(_)) => FAULTED,
        }
    })
}

#[no_mangle]
pub extern "C" fn shoop_rt_worklet_counter(id: u32) -> u32 {
    with_host(0, |host| {
        let counters = host.counters();
        match id {
            DISCONTINUITIES => counters.discontinuities,
            OVERRUNS => counters.overruns,
            MIDI_IN_DROPPED => counters.midi_in_dropped,
            MIDI_IN_LATE => counters.midi_in_late,
            MIDI_OUT_DROPPED => counters.midi_out_dropped,
            FRAMING_ERRORS => counters.framing_errors,
            _ => 0,
        }
    })
}
