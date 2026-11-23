//! Translate RT controls and observations to the packed loop state.
mod session;
mod window;
use crate::{bits::Bits, target::ProgramImage};
use rt_fpga_compiler::Identity;
use rt_fpga_reference::{
    fixed::Fixed,
    state::{ModePlan, ModeStep, apply_mode_command},
    transitions::{LoopId, LoopState, Mode, Operation, TransitionError},
};
pub use session::{ActiveProgram, FencedSwitch, ProgramSession};
use shoop_rt_protocol::{
    LoopMode,
    immediate::{
        ClearPlanned, Code, PlannedTransition, SwitchProgram, TransitionMode, TransitionPlan,
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
        capture_frames: words[4],
        cycles: words[5],
        content_revision: words[6],
        revision_watermark: words[7],
        capture_revision: words[7],
        last_pulse,
        repeat_sync: words[0] & (1 << 35) != 0,
    }
}

pub fn loop_plan(words: &[u64; 16]) -> TransitionPlan {
    let step = |i: usize| {
        let flags = (words[1] >> (32 + i * 5)) & 31;
        (flags & 1 != 0).then(|| PlannedTransition {
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
    if mask as u8 & 1 != 0 {
        words[1] &= !(1 << 32);
    }
    if mask as u8 & 2 != 0 {
        words[1] &= !(1 << 37);
    }
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
    let plans = apply_mode_command(
        &mut state,
        source,
        frame,
        ModePlan {
            now: plan.now.map(reference_mode),
            steps: [plan.first.map(step), plan.second.map(step)],
        },
    )
    .map_err(|error| match error {
        TransitionError::Exhausted | TransitionError::Capacity => Code::CaptureUnavailable,
        _ => Code::CannotApply,
    })?;
    words[0] = (words[0] & !(3 << 33)) | ((state.mode as u64) << 33);
    words[0] = (words[0] & !(1 << 56)) | (u64::from(plans[1].is_some()) << 56);
    words[2] = state.position;
    words[3] = state.content_length;
    words[4] = state.capture_frames;
    words[5] = state.cycles;
    words[6] = state.content_revision;
    words[7] = state.revision_watermark;
    words[1] &= (1 << 32) - 1;
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
            let flags = 1 | 2 | (op << 2) | (u64::from(retrigger) << 4);
            words[1] |= flags << (32 + 5 * i);
            words[10 + 3 * i] = pending.effective;
            words[11 + 3 * i] = pending.remaining_ticks;
            words[12 + 3 * i] = pos;
        }
    }
    Ok(if plans.iter().any(Option::is_some) {
        Code::Scheduled
    } else {
        Code::Applied
    })
}
