use serde::{Deserialize, Serialize};
use shoop_rt_protocol::immediate::{
    Command, Grab, PlannedTransition, TransitionMode, TransitionPlan,
};

pub const LOOPS: u32 = 64;
pub const CHANNELS: u32 = 2;
pub const QUANTUM: usize = 32;
pub const SAMPLE_RATE: u32 = 48_000;
pub const FPGA_CLOCK_HZ: u64 = 150_000_000;
pub const FPGA_CHUNK_FRAMES: usize = 32;
pub const FPGA_BLOCK_CHUNKS: usize = 32;
pub const FPGA_BLOCK_FRAMES: usize = FPGA_CHUNK_FRAMES * FPGA_BLOCK_CHUNKS;
pub const MEMORY_BYTES: usize = 16 * 1024 * 1024;
pub const GAIN: f32 = 1. / 128.;
pub const MIDI_BLOCK_EVENTS: u32 = 256;
pub const MIDI_INPUT_CAPACITY: usize = 40;
pub const MIDI_OUTPUT_CAPACITY: usize = MIDI_INPUT_CAPACITY * LOOPS as usize;
pub const MIDI_SCRATCH_EVENTS: usize = 65536;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Media {
    Audio,
    Midi,
    AudioMidi,
}
impl Media {
    pub fn audio(self) -> bool {
        self != Self::Midi
    }
    pub fn midi(self) -> bool {
        self != Self::Audio
    }
    pub fn audio_channels(self) -> usize {
        if self.audio() {
            CHANNELS as usize
        } else {
            0
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Pattern {
    Ddr,
    Stress,
    Lifecycle,
    Mixed,
    Grab,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Preset {
    Tiny,
    Quick,
    Full,
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
    pub media: Media,
    pub loops: u32,
    pub channels: u32,
    pub quantum: usize,
    pub sample_rate: u32,
    pub fpga_clock_hz: u64,
    pub fpga_chunk_frames: usize,
    pub fpga_block_chunks: usize,
    pub memory_bytes: usize,
    pub history_seconds: u32,
    pub smoothing_frames: u32,
    pub ramp_frames: u32,
    pub record_frames: usize,
    pub signal: String,
    pub midi_signal: String,
    pub midi_block_events: u32,
    pub midi_input_capacity: usize,
    pub midi_output_capacity: usize,
    pub midi_scratch_events: usize,
    pub callbacks: usize,
    pub events: Vec<Event>,
}
impl Fixture {
    pub fn new(preset: Preset, pattern: Pattern) -> Self {
        Self::with_media(preset, pattern, Media::Audio)
    }
    pub fn with_media(preset: Preset, pattern: Pattern, media: Media) -> Self {
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
        let span = FPGA_BLOCK_FRAMES / QUANTUM + 1;
        let iterations = 2;
        let callbacks = match pattern {
            Pattern::Ddr => {
                add(0, "cold-record-all", Action::Record, 0, LOOPS);
                add(span, "play-all", Action::Play, 0, LOOPS);
                add(span + 1, "stop-all", Action::Stop, 0, LOOPS);
                add(span + 2, "reuse-record-all", Action::Record, 0, LOOPS);
                add(span + 3, "stop-all", Action::Stop, 0, LOOPS);
                span + 5
            }
            Pattern::Stress => {
                add(0, "cold-record-all", Action::Record, 0, LOOPS);
                add(span, "play-all", Action::Play, 0, LOOPS);
                add(span + 1, "gain-all", Action::Gain(GAIN / 2.), 0, LOOPS);
                if preset == Preset::Tiny {
                    add(span + 9, "stop-all", Action::Stop, 0, LOOPS);
                    add(48, "reuse-record-all", Action::Record, 0, LOOPS);
                    add(50, "stop-all", Action::Stop, 0, LOOPS);
                    52
                } else {
                    add(2 * span - 1, "plan-all", Action::Planned, 0, LOOPS);
                    add(3 * span + 1, "stop-all", Action::Stop, 0, LOOPS);
                    let retake = 3 * span + 2;
                    add(retake, "reuse-record-all", Action::Record, 0, LOOPS);
                    add(retake + span, "play-all", Action::Play, 0, LOOPS);
                    add(
                        retake + span + 1,
                        "record-half",
                        Action::Record,
                        0,
                        LOOPS / 2,
                    );
                    add(
                        retake + 2 * span + 1,
                        "play-half",
                        Action::Play,
                        0,
                        LOOPS / 2,
                    );
                    add(retake + 2 * span + 2, "stop-all", Action::Stop, 0, LOOPS);
                    retake + 2 * span + 5
                }
            }
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
                let clock = 1024;
                add(0, "record-clock", Action::Record, 0, 1);
                add(clock, "play-clock", Action::Play, 0, 1);
                let start = 3 * clock;
                for i in 0..iterations {
                    add(start + i * clock, "grab-all", Action::Grab, 0, LOOPS);
                }
                let end = start + iterations * clock;
                add(end, "stop-all", Action::Stop, 0, LOOPS);
                end + span
            }
        };
        Self {
            version: 5,
            preset,
            pattern,
            media,
            loops: LOOPS,
            channels: media.audio_channels() as u32,
            quantum: QUANTUM,
            sample_rate: SAMPLE_RATE,
            fpga_clock_hz: FPGA_CLOCK_HZ,
            fpga_chunk_frames: FPGA_CHUNK_FRAMES,
            fpga_block_chunks: FPGA_BLOCK_CHUNKS,
            memory_bytes: MEMORY_BYTES,
            history_seconds: if pattern == Pattern::Grab { 1 } else { 0 },
            smoothing_frames: 0,
            ramp_frames: 0,
            record_frames: if pattern == Pattern::Grab {
                32768
            } else {
                span * QUANTUM
            },
            signal: "stereo-saw-v1".into(),
            midi_signal: "burst-quiet-cc-v1: 40 messages every 16th quantum, 16 in the next seven, then eight silent quanta; ties and offsets 0/31".into(),
            midi_block_events: MIDI_BLOCK_EVENTS,
            midi_input_capacity: MIDI_INPUT_CAPACITY,
            midi_output_capacity: MIDI_OUTPUT_CAPACITY,
            midi_scratch_events: MIDI_SCRATCH_EVENTS,
            callbacks,
            events,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidiPacket {
    pub offset: u32,
    pub bytes: [u8; 3],
}

pub fn midi_input(frame: u64, packets: &mut Vec<MidiPacket>) {
    packets.clear();
    let quantum = frame / QUANTUM as u64;
    let count = match quantum % 16 {
        0 => 40,
        1..=7 => 16,
        _ => 0,
    };
    for index in 0..count {
        let offset = (index * (QUANTUM - 1) / (count - 1)) as u32;
        packets.push(MidiPacket {
            offset,
            bytes: [
                0xb0 | (quantum % 16) as u8,
                1 + (index % 31) as u8,
                ((quantum * 17 + index as u64 * 3) % 128) as u8,
            ],
        });
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
            let f = Fixture::new(Preset::Full, pattern);
            assert_eq!((f.loops, f.channels, f.quantum), (64, 2, 32));
            assert_eq!(
                (f.version, f.fpga_chunk_frames, f.fpga_block_chunks),
                (5, 32, 32)
            );
            assert!(f.record_frames > f.fpga_chunk_frames * f.fpga_block_chunks);
            assert!(f.events.windows(2).all(|v| v[0].callback < v[1].callback));
            assert!(f.events.last().unwrap().callback < f.callbacks);
            assert_eq!(
                serde_json::to_string(&f).unwrap(),
                serde_json::to_string(&Fixture::new(Preset::Full, pattern)).unwrap()
            );
        }
        let commands = Fixture::new(Preset::Full, Pattern::Lifecycle).events[0].commands();
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

    #[shoop_test]
    fn compact_stress_preserves_the_dense_peak_prefix_and_cold_geometry() {
        let full = Fixture::with_media(Preset::Full, Pattern::Lifecycle, Media::AudioMidi);
        assert_eq!(full.callbacks, 298);
        for preset in [Preset::Tiny, Preset::Quick] {
            let compact = Fixture::with_media(preset, Pattern::Stress, Media::AudioMidi);
            let prefix = |fixture: &Fixture| {
                serde_json::to_value(
                    fixture
                        .events
                        .iter()
                        .filter(|event| event.callback < 42)
                        .collect::<Vec<_>>(),
                )
                .unwrap()
            };
            assert_eq!(prefix(&compact), prefix(&full));
            assert_eq!(compact.record_frames, 1056);
            assert_eq!(compact.events.last().unwrap().label, "stop-all");
        }
    }
}
