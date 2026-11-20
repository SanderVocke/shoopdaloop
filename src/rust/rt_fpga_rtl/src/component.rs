//! Component instruction and state values for explicit simulation fixtures.
use alloc::vec;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub slot: u32,
    pub generation: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Value {
    Word(u64),
    Ramp(Ramp),
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ramp {
    pub start: i32,
    pub target: i32,
    pub duration: u32,
    pub elapsed: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u16)]
pub enum ArithmeticMode {
    Multiply = 0,
    LeftBalance = 1,
    RightBalance = 2,
    OneMinus = 3,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Instruction {
    ProcessMidi {
        phase: u16,
        loop_id: Option<Identity>,
        context: u16,
    },
    ProcessHistorySmoothing {
        loop_id: Identity,
        control: u16,
        source: u16,
        destination: u16,
        history: u16,
    },
    ProcessSmoothing {
        loop_id: Identity,
        control: u16,
        source: u16,
    },
    ProcessConstant {
        value: i32,
        destination: u16,
    },
    ProcessArithmetic {
        left: u16,
        right: u16,
        destination: u16,
        mode: ArithmeticMode,
    },
    ProcessInput {
        port: u16,
        destination: u16,
    },
    ProcessLogicalPlayback {
        loop_id: Identity,
        view: u16,
        destination: u16,
    },
    ProcessLogicalCapture {
        loop_id: Identity,
        view: u16,
        source: u16,
    },
    ProcessGain {
        ramp: u16,
        source: u16,
        destination: u16,
    },
    ProcessMix {
        left: u16,
        right: u16,
        destination: u16,
    },
    ProcessOutput {
        source: u16,
        port: u16,
    },
    ProcessSilence {
        destination: u16,
    },
}

pub fn value_words(value: Value) -> [u32; 5] {
    match value {
        Value::Word(v) => [v as u32, (v >> 32) as u32, 0, 0, 0],
        Value::Ramp(r) => [r.start as u32, r.target as u32, r.duration, r.elapsed, 1],
    }
}
pub fn instruction_words(value: Instruction) -> crate::Result<[u32; 5]> {
    let (opcode, args) = match value {
        Instruction::ProcessMidi {
            phase,
            loop_id,
            context,
        } => {
            let id = loop_id.unwrap_or(Identity {
                slot: 0,
                generation: 0,
            });
            (
                14,
                vec![
                    u16::try_from(id.slot)?,
                    id.generation as u16,
                    (id.generation >> 16) as u16,
                    context,
                    phase,
                ],
            )
        }
        Instruction::ProcessHistorySmoothing {
            loop_id,
            control,
            source,
            destination,
            history,
        } => (
            13,
            vec![
                u16::try_from(loop_id.slot)?,
                control,
                source,
                destination,
                history,
            ],
        ),
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
                u16::try_from(loop_id.slot)?,
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
                u16::try_from(loop_id.slot)?,
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
                u16::try_from(loop_id.slot)?,
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
