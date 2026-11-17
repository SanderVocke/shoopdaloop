use serde::{Deserialize, Serialize};
use shoop_rt_protocol::immediate::{
    Command, Grab, PlannedTransition, TransitionMode, TransitionPlan,
};

pub const LOOPS: u32 = 64;
pub const CHANNELS: u32 = 2;
pub const QUANTUM: usize = 32;
pub const SAMPLE_RATE: u32 = 48_000;
pub const FPGA_CLOCK_HZ: u64 = 150_000_000;
pub const MEMORY_BYTES: usize = 16 * 1024 * 1024;
pub const GAIN: f32 = 1. / 128.;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Pattern {
    Lifecycle,
    Mixed,
    Grab,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Preset {
    Smoke,
    Standard,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    Record,
    Play,
    Stop,
    Gain(f32),
    Planned,
    Grab,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub callback: usize,
    pub label: String,
    pub action: Action,
    pub first: u32,
    pub count: u32,
}
impl Event {
    pub fn commands(&self) -> Vec<Command> {
        (self.first..self.first + self.count)
            .map(|id| match self.action {
                Action::Record => Command::Transition(id, TransitionMode::Recording.into()),
                Action::Play => Command::Transition(id, TransitionMode::Playing.into()),
                Action::Stop => Command::Transition(id, TransitionMode::Stopped.into()),
                Action::Gain(value) => Command::LoopGain(id, value),
                Action::Planned => Command::Transition(
                    id,
                    TransitionPlan {
                        now: None,
                        first: Some(PlannedTransition {
                            mode: TransitionMode::Playing,
                            skip: 0,
                        }),
                        second: Some(PlannedTransition {
                            mode: TransitionMode::Playing,
                            skip: 0,
                        }),
                    },
                ),
                Action::Grab => Command::Grab(
                    id,
                    Grab {
                        sync: Some(0),
                        reverse_start_cycle: Some(1),
                        cycles_length: Some(1),
                        go_to_cycle: Some(0),
                        mode: TransitionMode::Playing,
                    },
                ),
            })
            .collect()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Fixture {
    pub version: u32,
    pub preset: Preset,
    pub pattern: Pattern,
    pub loops: u32,
    pub channels: u32,
    pub quantum: usize,
    pub sample_rate: u32,
    pub fpga_clock_hz: u64,
    pub memory_bytes: usize,
    pub history_seconds: u32,
    pub smoothing_frames: u32,
    pub ramp_frames: u32,
    pub signal: String,
    pub callbacks: usize,
    pub events: Vec<Event>,
}
impl Fixture {
    pub fn new(preset: Preset, pattern: Pattern) -> Self {
        let mut events = Vec::new();
        let mut add = |callback, label: &str, action, first, count| {
            events.push(Event {
                callback,
                label: label.into(),
                action,
                first,
                count,
            })
        };
        let span = 32;
        let iterations = match preset {
            Preset::Smoke => 2,
            Preset::Standard => 8,
        };
        let callbacks = match pattern {
            Pattern::Lifecycle => {
                for i in 0..iterations {
                    let base = i * (4 * span + 1);
                    add(
                        base,
                        if i == 0 {
                            "cold-record-all"
                        } else {
                            "reuse-record-all"
                        },
                        Action::Record,
                        0,
                        LOOPS,
                    );
                    add(base + span, "play-all", Action::Play, 0, LOOPS);
                    add(
                        base + span + 1,
                        "gain-all",
                        Action::Gain(if i % 2 == 0 { GAIN / 2. } else { GAIN }),
                        0,
                        LOOPS,
                    );
                    add(base + 2 * span - 1, "plan-all", Action::Planned, 0, LOOPS);
                    add(base + 4 * span, "stop-all", Action::Stop, 0, LOOPS);
                }
                iterations * (4 * span + 1) + 32
            }
            Pattern::Mixed => {
                add(0, "cold-record-all", Action::Record, 0, LOOPS);
                add(span, "play-all", Action::Play, 0, LOOPS);
                let mut base = 2 * span;
                for i in 0..iterations {
                    let first = if i % 2 == 0 { 0 } else { LOOPS / 2 };
                    add(base, "record-half", Action::Record, first, LOOPS / 2);
                    let length = span + [0, 1, 3][i % 3];
                    add(base + length, "play-half", Action::Play, first, LOOPS / 2);
                    base += length + span;
                }
                add(base, "stop-all", Action::Stop, 0, LOOPS);
                base + span
            }
            Pattern::Grab => {
                add(0, "record-clock", Action::Record, 0, 1);
                add(span, "play-clock", Action::Play, 0, 1);
                let start = SAMPLE_RATE as usize / QUANTUM + span;
                for i in 0..iterations {
                    add(start + i * span, "grab-all", Action::Grab, 0, LOOPS);
                }
                let end = start + iterations * span;
                add(end, "stop-all", Action::Stop, 0, LOOPS);
                end + span
            }
        };
        Self {
            version: 1,
            preset,
            pattern,
            loops: LOOPS,
            channels: CHANNELS,
            quantum: QUANTUM,
            sample_rate: SAMPLE_RATE,
            fpga_clock_hz: FPGA_CLOCK_HZ,
            memory_bytes: MEMORY_BYTES,
            history_seconds: 1,
            smoothing_frames: 0,
            ramp_frames: 0,
            signal: "stereo-saw-v1".into(),
            callbacks,
            events,
        }
    }
}

pub fn input(channel: usize, frame: u64) -> f32 {
    let value = if channel == 0 {
        frame.wrapping_mul(17) % 251
    } else {
        frame.wrapping_mul(31).wrapping_add(47) % 239
    };
    (value as f32 - 128.) / 1024.
}

pub fn cycles_us(cycles: u64) -> f64 {
    cycles as f64 * 1_000_000. / FPGA_CLOCK_HZ as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;
    #[shoop_test]
    fn fixture_preserves_layout_and_replays_identically() {
        for pattern in [Pattern::Lifecycle, Pattern::Mixed, Pattern::Grab] {
            let f = Fixture::new(Preset::Smoke, pattern);
            assert_eq!((f.loops, f.channels, f.quantum), (64, 2, 32));
            assert!(f.events.windows(2).all(|v| v[0].callback < v[1].callback));
            assert!(f.events.last().unwrap().callback < f.callbacks);
            assert_eq!(
                serde_json::to_string(&f).unwrap(),
                serde_json::to_string(&Fixture::new(Preset::Smoke, pattern)).unwrap()
            );
        }
        let commands = Fixture::new(Preset::Smoke, Pattern::Lifecycle).events[0].commands();
        assert_eq!(
            commands
                .chunks(shoop_rt_protocol::immediate::COMMANDS)
                .map(|c| c.len())
                .collect::<Vec<_>>(),
            [32, 32]
        );
        assert_eq!(cycles_us(100_000), 666.6666666666666);
        assert_ne!(input(0, 0), input(1, 0));
    }
}
