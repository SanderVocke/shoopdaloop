//! Signal program encoding shared by physical and simulated transports.
use crate::image::{Instruction, Value};
pub fn value_words(value: Value) -> [u32; 5] {
    match value {
        Value::Word(v) => [v as u32, (v >> 32) as u32, 0, 0, 0],
        Value::Ramp(r) => [r.start as u32, r.target as u32, r.duration, r.elapsed, 1],
    }
}
pub fn instruction_words(value: Instruction) -> Result<[u32; 5], rt_runtime::Error> {
    let (opcode, args) = match value {
        Instruction::ProcessInput { port, destination } => (0, vec![port, destination]),
        Instruction::ProcessGain {
            ramp,
            source,
            destination,
        } => (2, vec![ramp, source, destination]),
        Instruction::ProcessSmoothing {
            loop_id,
            control,
            source,
        } => (
            11,
            vec![
                u16::try_from(loop_id.slot).map_err(|_| rt_runtime::Error::InvalidId)?,
                loop_id.generation as u16,
                (loop_id.generation >> 16) as u16,
                control,
                source,
            ],
        ),
        Instruction::ProcessConstant { value, destination } => (
            9,
            vec![value as u16, (value as u32 >> 16) as u16, destination],
        ),
        Instruction::ProcessArithmetic {
            left,
            right,
            destination,
            mode,
        } => (10, vec![left, right, destination, mode as u16]),
        Instruction::ProcessMix {
            left,
            right,
            destination,
        } => (3, vec![left, right, destination]),
        Instruction::ProcessOutput { source, port } => (5, vec![source, port]),
        Instruction::ProcessSilence { destination } => (8, vec![destination]),
        Instruction::ProcessLogicalPlayback {
            loop_id,
            view,
            destination,
        } => (
            6,
            vec![
                u16::try_from(loop_id.slot).map_err(|_| rt_runtime::Error::InvalidId)?,
                loop_id.generation as u16,
                (loop_id.generation >> 16) as u16,
                view,
                destination,
            ],
        ),
        Instruction::ProcessLogicalCapture {
            loop_id,
            view,
            source,
        } => (
            7,
            vec![
                u16::try_from(loop_id.slot).map_err(|_| rt_runtime::Error::InvalidId)?,
                loop_id.generation as u16,
                (loop_id.generation >> 16) as u16,
                view,
                source,
            ],
        ),
    };
    let packed = args
        .iter()
        .enumerate()
        .fold(opcode as u128, |word, (i, &arg)| {
            word | (u128::from(arg) << (8 + 16 * i))
        });
    Ok([
        packed as u32,
        (packed >> 32) as u32,
        (packed >> 64) as u32,
        0,
        0,
    ])
}
