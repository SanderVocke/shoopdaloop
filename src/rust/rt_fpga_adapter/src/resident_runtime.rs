//! Signal command construction; node/loop validity and current configuration are checked at delivery.
use crate::{audio_actions, audio_image::Transfer, protocol::put, resident};
use rt_audio_math::{Q824, Sample};
use rt_runtime::{Error, NodeId};
use shoop_loops::{LoopId, audio::Smoothing};
use shoop_rt_engine::{EngineOp, external::SignalKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u8)]
pub enum Parameter {
    Gain = 0,
    Balance = 1,
    Mute = 2,
    Presence = 3,
    Bank = 4,
    Monitor = 5,
    Stereo = 6,
}
pub const PARAMETERS: u32 = 7;
pub fn key(id: NodeId, parameter: Parameter) -> Result<u16, Error> {
    id.slot()
        .checked_mul(PARAMETERS)
        .and_then(|n| n.checked_add(parameter as u32))
        .and_then(|n| u16::try_from(n).ok())
        .ok_or(Error::Capacity)
}
fn signal(id: NodeId, opcode: u8) -> Transfer {
    let mut t = Transfer {
        opcode: audio_actions::PARAMETER,
        state: false,
        address: 0,
        words: [0; 20],
    };
    put(&mut t.words, 0, 32, u64::from(id.slot()));
    put(&mut t.words, 32, 32, u64::from(id.generation()));
    put(&mut t.words, 64, 8, u64::from(opcode));
    t
}
fn float(t: &mut Transfer, value: f32, balance: bool) {
    put(&mut t.words, 72, 32, u64::from(value.to_bits()));
    if !value.is_finite() || value.abs() >= 128. || (balance && !(-1.0..=1.0).contains(&value)) {
        put(&mut t.words, resident::DEFERRED_ERROR, 8, 2);
    } else {
        put(
            &mut t.words,
            104,
            32,
            u64::from(Q824::from_f32(value).0 as u32),
        );
    }
}
pub fn processor(id: NodeId, operation: EngineOp) -> Transfer {
    let mut t = signal(id, 255);
    match operation {
        EngineOp::Gain(value) | EngineOp::Balance(value) => {
            let balance = matches!(operation, EngineOp::Balance(_));
            put(&mut t.words, 64, 8, u64::from(balance));
            float(&mut t, value, balance);
        }
        EngineOp::Mute(enabled) | EngineOp::Presence(enabled) | EngineOp::Monitor(enabled) => {
            let op = match operation {
                EngineOp::Mute(_) => 2,
                EngineOp::Presence(_) => 3,
                _ => 5,
            };
            put(&mut t.words, 64, 8, op);
            put(&mut t.words, 72, 1, u64::from(enabled));
        }
        EngineOp::Crossfade => put(&mut t.words, 64, 8, 4),
        EngineOp::InputSource { epoch, changed } => {
            put(&mut t.words, 64, 8, 9);
            put(&mut t.words, 72, 64, epoch.0);
            put(&mut t.words, 136, 1, u64::from(changed));
        }
        EngineOp::DiscardConfiguration { token } => {
            put(&mut t.words, 64, 8, 10);
            put(&mut t.words, 72, 64, token);
        }
        EngineOp::Configure(c) => {
            put(&mut t.words, 64, 8, 11);
            put(&mut t.words, 72, 32, u64::from(c.generation));
            put(&mut t.words, 104, 64, c.token);
            put(&mut t.words, 168, 64, c.epoch.0);
            for (offset, value) in [
                (232, c.inputs),
                (240, c.outputs),
                (248, c.bank),
                (256, c.send_bank),
                (264, c.return_bank),
            ] {
                put(&mut t.words, offset, 8, u64::from(value));
            }
            put(&mut t.words, 272, 32, u64::from(c.gain.to_bits()));
            put(&mut t.words, 304, 32, u64::from(c.balance.to_bits()));
            put(&mut t.words, 336, 1, u64::from(c.monitoring));
            put(&mut t.words, 337, 1, u64::from(c.muted));
            if c.processor.is_some()
                || c.recorded_state.is_some()
                || c.gain >= 128.
                || c.generation == 0
            {
                put(&mut t.words, resident::DEFERRED_ERROR, 8, 2);
            }
            if c.token == 0
                || c.bank > 1
                || c.send_bank > 1
                || c.return_bank > 1
                || !c.gain.is_finite()
                || c.gain < 0.
                || !(-1.0..=1.0).contains(&c.balance)
            {
                put(&mut t.words, 488, 8, 2);
            }
            if c.gain.is_finite() && c.gain.abs() < 128. {
                put(
                    &mut t.words,
                    338,
                    32,
                    u64::from(Q824::from_f32(c.gain).0 as u32),
                );
            }
            if c.balance.is_finite() && c.balance.abs() < 128. {
                put(
                    &mut t.words,
                    370,
                    32,
                    u64::from(Q824::from_f32(c.balance).0 as u32),
                );
            }
        }
        EngineOp::ReadStart { .. } => put(&mut t.words, 64, 8, 12),
        EngineOp::ReadStop => put(&mut t.words, 64, 8, 13),
        _ => {}
    }
    t
}
pub fn loop_parameter(target: LoopId, value: f32, balance: bool) -> Transfer {
    let mut t = signal(
        NodeId::from_parts(target.slot(), target.generation()).expect("valid loop identity"),
        if balance { 7 } else { 6 },
    );
    float(&mut t, value, balance);
    t
}
pub fn smoothing(target: LoopId, smoothing: Smoothing) -> Transfer {
    let mut t = signal(
        NodeId::from_parts(target.slot(), target.generation()).expect("valid loop identity"),
        8,
    );
    if let Smoothing::LinearRestart { frames } = smoothing {
        put(&mut t.words, 72, 32, u64::from(frames));
        put(&mut t.words, 104, 1, 1);
    }
    t
}
pub fn metadata(id: NodeId, kind: SignalKind) -> Result<Transfer, Error> {
    let (kind, channels, ramp, slot) = match kind {
        SignalKind::Playback { slot, channels, .. } => (0, channels, 0, u64::from(slot)),
        SignalKind::Capture { slot, channels, .. } => (1, channels, 0, u64::from(slot)),
        SignalKind::StripInput {
            slot,
            channels,
            ramp,
        } => (2, channels, ramp, slot as u64),
        SignalKind::StripOutput {
            slot,
            channels,
            ramp,
        } => (3, channels, ramp, slot as u64),
        SignalKind::Fader {
            slot,
            channels,
            ramp,
        } => (4, channels, ramp, slot as u64),
        SignalKind::BusInput { slot, channels } => (5, channels, 0, slot as u64),
        SignalKind::Bus {
            slot,
            channels,
            ramp,
        } => (6, channels, ramp, slot as u64),
        SignalKind::Reader => (7, 0, 0, 0),
    };
    if channels > 2 || slot > u64::from(u32::MAX) {
        return Err(Error::Capacity);
    }
    let mut t = Transfer {
        opcode: 51,
        state: false,
        address: 0,
        words: [0; 20],
    };
    put(&mut t.words, 0, 32, u64::from(id.slot()));
    put(&mut t.words, 32, 32, u64::from(id.generation()));
    put(&mut t.words, 64, 4, kind);
    put(&mut t.words, 68, 8, channels as u64);
    put(&mut t.words, 76, 32, u64::from(ramp));
    put(&mut t.words, 108, 32, slot);
    Ok(t)
}
pub fn policy(id: NodeId, generation: u32, enabled: bool) -> Transfer {
    let mut t = Transfer {
        opcode: 51,
        state: false,
        address: 0,
        words: [0; 20],
    };
    put(&mut t.words, 0, 32, u64::from(id.slot()));
    put(&mut t.words, 32, 32, u64::from(id.generation()));
    put(&mut t.words, 64, 32, u64::from(generation));
    put(&mut t.words, 96, 1, u64::from(enabled));
    put(&mut t.words, 632, 8, 1);
    t
}
