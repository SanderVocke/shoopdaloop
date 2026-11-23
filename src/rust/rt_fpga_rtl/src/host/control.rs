//! Translate RT controls and observations to the packed loop state.
mod session;
mod window;
use crate::{
    bits::Bits,
    target::{COMPOSITE_LOOP, ProgramImage},
};
use rt_fpga_compiler::{
    Identity,
    loading::{ClockState, CompositeCut, Layout},
    shoop_graph::CompositeKind,
};
use rt_fpga_reference::{
    fixed::Fixed,
    state::{
        ModePlan, ModeStep, apply_mode_command, prepare_composite_iteration,
        prepare_composite_mode_plan,
    },
    transitions::{LoopId, LoopState, Mode, Operation, TransitionError},
};
pub use session::{ActiveProgram, FencedSwitch, ProgramSession};
use shoop_rt_protocol::{
    LoopMode, LoopSlotId, SlotId, Telemetry,
    immediate::{
        ClearPlanned, Code, IterationTransition, PlannedTransition, SwitchProgram, TransitionMode,
        TransitionPlan,
    },
};
pub use window::ControlWindow;

pub fn descriptor(image: &ProgramImage) -> SwitchProgram {
    let program = &image.compiled;
    SwitchProgram {
        revision: image.layout.revision,
        region: u16::from(image.layout.region),
        instructions: program.instructions.len() as u16,
        sync_count: program.sync_count as u16,
        states: program.state.len() as u16,
        jobs: program.jobs_per_quantum as u16,
        buffers: program.resources.buffers as u16,
    }
}

pub fn gain_value(gain: f32) -> Bits {
    let fixed = Fixed::from_f32(gain).0 as u32 as u64;
    let mut bits = Bits::zero(129);
    bits.set(128, 1, 1).set(0, 32, fixed).set(32, 32, fixed);
    bits
}

pub fn reference_mode(mode: TransitionMode) -> Mode {
    match mode {
        TransitionMode::Stopped => Mode::Stopped,
        TransitionMode::Playing => Mode::Playing,
        TransitionMode::Recording => Mode::Recording,
    }
}

pub fn protocol_mode(mode: Mode) -> LoopMode {
    match mode {
        Mode::Stopped => LoopMode::Stopped,
        Mode::Playing => LoopMode::Playing,
        Mode::Recording => LoopMode::Recording,
    }
}

pub fn loop_state(id: Identity, words: &[u64; 16], last_pulse: Option<u64>) -> LoopState {
    let composite = words[1] & COMPOSITE_LOOP != 0;
    LoopState {
        id: LoopId {
            slot: id.slot,
            generation: id.generation,
        },
        mode: match (words[0] >> 33) & 3 {
            1 => Mode::Playing,
            2 => Mode::Recording,
            _ => Mode::Stopped,
        },
        position: words[2],
        content_length: words[3],
        capture_frames: if composite { 0 } else { words[4] },
        cycles: words[5],
        content_revision: if composite { 0 } else { words[6] },
        revision_watermark: words[7],
        capture_revision: words[7],
        last_pulse,
        repeat_sync: words[0] & (1 << 35) != 0,
    }
}

pub fn composite_telemetry(
    id: Identity,
    frame: u64,
    position: u64,
    words: &[u64; 16],
    active: &Layout,
    staged: Option<(&Layout, bool)>,
) -> Option<Telemetry> {
    let composite = active.composite_schedule(id)?;
    let pending = staged
        .and_then(|(layout, _)| layout.composite_schedule(id))
        .filter(|plan| plan.version != composite.version);
    let state = loop_state(id, words, None);
    let unit = u64::from(composite.unit_frames);
    Some(Telemetry::Composite {
        loop_id: LoopSlotId(SlotId::new(id.slot, id.generation)),
        frame,
        mode: protocol_mode(state.mode),
        position,
        length: state.content_length,
        active_version: composite.version,
        pending_version: pending.map(|plan| plan.version),
        pending_admitted: pending.is_some() && staged.is_some_and(|(_, ready)| ready),
        iteration: (position / unit).min((state.content_length / unit).saturating_sub(1)),
        passes: state.cycles,
        waiting_repeat: words[1] & (1 << 42) != 0,
    })
}

pub fn composite_cut_status<E>(
    cut: &CompositeCut,
    words: &[u64; 16],
    read_clock: impl FnMut(Identity) -> Result<Option<ClockState>, E>,
) -> Result<Code, E> {
    if !loop_identity_valid(cut.old_slot, words) || (cut.require_empty && words[3] != 0) {
        return Ok(Code::CannotApply);
    }
    let state = loop_state(cut.old_slot, words, None);
    if has_loop_plan(words) && !cut.preserves_transport_at_finish(state.at_end()) {
        return Ok(if cut.waits_for_control() {
            Code::Scheduled
        } else {
            Code::CannotApply
        });
    }
    let restart_ready = cut.restart_ready(state.repeat_sync, state.at_end(), read_clock)?;
    let waited = words[1] & (1 << 42) != 0;
    Ok(match (words[0] >> 33) & 3 {
        0 => Code::Applied,
        1 if cut.allows_playing(words[2], words[3], restart_ready, waited) => Code::Applied,
        1 if cut.waits_for_boundary(words[2], words[3], restart_ready, waited) => Code::Scheduled,
        _ => Code::CannotApply,
    })
}

pub fn restart_clock_state<E>(
    id: Identity,
    frame: u64,
    words: [u64; 4],
    mut read_plan_timing: impl FnMut(u32) -> Result<[u64; 2], E>,
) -> Result<Option<ClockState>, E> {
    if !loop_identity_valid(id, &words) || (words[0] >> 33) & 3 == 3 {
        return Ok(None);
    }
    for index in 0..2 {
        let flags = (words[1] >> (32 + 5 * index)) & 31;
        if flags & 1 == 0 {
            continue;
        }
        let [effective, ticks] = read_plan_timing(10 + 3 * index)?;
        if effective <= frame && (flags & 2 == 0 || ticks == 0) {
            return Ok(None);
        }
    }
    let playing = (words[0] >> 33) & 3 == 1;
    Ok(Some(ClockState {
        kind: (words[1] & COMPOSITE_LOOP != 0).then(|| {
            if words[0] & (1 << 62) == 0 {
                CompositeKind::Regular
            } else {
                CompositeKind::Explicit
            }
        }),
        playing,
        at_end: playing && words[3] != 0 && words[2] == words[3],
        repeat_sync: words[0] & (1 << 35) != 0,
    }))
}

pub fn prepare_composite_restart(
    destination: Identity,
    old: &[u64; 16],
    next: &mut [u64; 16],
    quantum: u32,
) -> Code {
    if !cut_loop_valid(destination, next)
        || next[1] & COMPOSITE_LOOP == 0
        || next[3] < u64::from(quantum)
    {
        return Code::CannotApply;
    }
    const MODE_AND_REPEAT: u64 = 7 << 33;
    const WAIT: u64 = 1 << 42;
    next[0] = (next[0] & !MODE_AND_REPEAT) | (old[0] & MODE_AND_REPEAT);
    next[1] = (next[1] & !WAIT) | (old[1] & WAIT);
    next[2] = next[3];
    next[5] = old[5];
    next[8] = CompositeCut::restart_previous(old[8], old[3], next[3], quantum);
    Code::Applied
}

pub fn prepare_composite_retirement(id: Identity, words: &mut [u64; 16], frame: u64) -> Code {
    let mode = (words[0] >> 33) & 3;
    if !loop_identity_valid(id, words) || words[1] & COMPOSITE_LOOP == 0 || mode > 1 {
        return Code::CannotApply;
    }
    if mode == 0 && cut_loop_valid(id, words) {
        return Code::Applied;
    }
    let state = loop_state(id, words, None);
    match apply_plan(words, state, None, frame, TransitionMode::Stopped.into()) {
        Ok(_) => Code::Scheduled,
        Err(code) => code,
    }
}

fn cut_loop_valid(slot: Identity, words: &[u64]) -> bool {
    loop_identity_valid(slot, words) && !has_loop_plan(words)
}

fn has_loop_plan(words: &[u64]) -> bool {
    words[1] & ((1 << 32) | (1 << 37)) != 0
}

fn loop_identity_valid(slot: Identity, words: &[u64]) -> bool {
    words[0] & 1 != 0 && (words[0] >> 1) as u32 == slot.generation
}

pub fn loop_plan(words: &[u64; 16]) -> TransitionPlan {
    let step = |i: usize| {
        let flags = (words[1] >> (32 + i * 5)) & 31;
        (flags & 3 == 3).then(|| PlannedTransition {
            mode: match (flags >> 2) & 3 {
                0 => TransitionMode::Stopped,
                2 => TransitionMode::Recording,
                _ => TransitionMode::Playing,
            },
            skip: words[11 + 3 * i].min(u64::from(u32::MAX)) as u32,
        })
    };
    TransitionPlan {
        now: None,
        first: step(0),
        second: step(1),
    }
}

pub fn clear_plan(words: &mut [u64; 16], mask: ClearPlanned) {
    let composite_timing = words[1] & (COMPOSITE_LOOP | (1 << 44)) != 0;
    if mask as u8 & 1 != 0 && (!composite_timing || words[1] & (1 << 33) != 0) {
        words[1] &= !(1 << 32);
    }
    if mask as u8 & 2 != 0 && (!composite_timing || words[1] & (1 << 38) != 0) {
        words[1] &= !(1 << 37);
    }
}

pub fn set_repeat_sync(words: &mut [u64; 16], enabled: bool) {
    words[0] = (words[0] & !(1 << 35)) | (u64::from(enabled) << 35);
}

/// Updates the copied loop record, or returns a refusal before changing it.
pub fn apply_plan(
    words: &mut [u64; 16],
    mut state: LoopState,
    source: Option<LoopId>,
    frame: u64,
    plan: TransitionPlan,
) -> Result<Code, Code> {
    let step = |step: PlannedTransition| ModeStep {
        mode: reference_mode(step.mode),
        skip: step.skip,
    };
    let plan = ModePlan {
        now: plan.now.map(reference_mode),
        steps: [plan.first.map(step), plan.second.map(step)],
    };
    let composite = words[1] & COMPOSITE_LOOP != 0;
    let queued = composite || words[1] & (1 << 44) != 0;
    let plans = if queued {
        prepare_composite_mode_plan(state, source, frame, plan, composite)
    } else {
        apply_mode_command(&mut state, source, frame, plan)
    }
    .map_err(|error| match error {
        TransitionError::Exhausted | TransitionError::Capacity => Code::CaptureUnavailable,
        TransitionError::Unsupported => Code::Unsupported,
        _ => Code::CannotApply,
    })?;
    if !queued {
        words[0] = (words[0] & !(3 << 33)) | ((state.mode as u64) << 33);
        words[2] = state.position;
        words[3] = state.content_length;
        words[4] = state.capture_frames;
        words[5] = state.cycles;
        words[6] = state.content_revision;
        words[7] = state.revision_watermark;
    }
    store_plans(words, plans);
    Ok(if plans.iter().flatten().any(|plan| plan.on_pulse) {
        Code::Scheduled
    } else {
        Code::Applied
    })
}

pub fn apply_iteration(
    words: &mut [u64; 16],
    state: LoopState,
    frame: u64,
    request: IterationTransition,
) -> Result<Code, Code> {
    if words[1] & COMPOSITE_LOOP == 0 {
        return Err(Code::TargetUnavailable);
    }
    let unit = u32::try_from(words[6]).map_err(|_| Code::CannotApply)?;
    let plan = prepare_composite_iteration(
        state,
        unit,
        frame,
        request.iteration,
        request.retrigger,
        request.seek,
    )
    .map_err(|error| match error {
        TransitionError::InvalidId => Code::TargetUnavailable,
        TransitionError::InvalidFrame => Code::InvalidValue,
        _ => Code::CannotApply,
    })?;
    store_plans(words, [Some(plan), None]);
    Ok(Code::Applied)
}

fn store_plans(
    words: &mut [u64; 16],
    plans: [Option<rt_fpga_reference::state::PlannedTransition>; 2],
) {
    let newer = plans[1]
        .is_some_and(|later| plans[0].is_none_or(|earlier| later.sequence >= earlier.sequence));
    words[0] = (words[0] & !(1 << 56)) | (u64::from(newer) << 56);
    words[1] &= !(0x3ff << 32);
    for (i, pending) in plans.iter().enumerate() {
        if let Some(pending) = pending {
            let (op, pos, retrigger) = match pending.operation {
                Operation::Stop => (0, 0, false),
                Operation::Play {
                    position,
                    retrigger,
                } => (1, position, retrigger),
                Operation::Record => (2, 0, false),
                Operation::Seek {
                    position,
                    retrigger,
                } => (3, position, retrigger),
            };
            let flags =
                1 | (u64::from(pending.on_pulse) << 1) | (op << 2) | (u64::from(retrigger) << 4);
            words[1] |= flags << (32 + 5 * i);
            words[10 + 3 * i] = pending.effective;
            words[11 + 3 * i] = pending.remaining_ticks;
            words[12 + 3 * i] = pos;
        }
    }
}
