//! Independent scalar sample and metadata services for the diagnostic memory port.
use super::{Active, Control, Stable};
use crate::{model::Model, random::Random};
#[derive(Default)]
struct Channel {
    pending: Option<(u64, u32, bool, bool)>,
    held: Stable<(u64, u64, u64)>,
}
pub struct Legacy {
    pub words: Vec<u32>,
    base: u64,
    random: Random,
    sample: Channel,
    metadata: Channel,
    pub reads: u64,
    pub writes: u64,
}
impl Legacy {
    pub fn new(words: usize, base: u64, seed: u64) -> Self {
        Self {
            words: vec![0xa5a5a5a5; words],
            base,
            random: Random::new(seed),
            sample: Channel::default(),
            metadata: Channel::default(),
            reads: 0,
            writes: 0,
        }
    }
    pub fn rebase(&mut self, base: u64) {
        self.base = base;
    }
    pub fn active(&self) -> Option<Active> {
        self.sample
            .pending
            .map(|(_, _, error, write)| Active {
                write,
                error,
                index: 0,
            })
            .or_else(|| {
                self.metadata.pending.map(|(_, _, error, write)| Active {
                    write,
                    error,
                    index: 0,
                })
            })
    }
    pub fn drive(&mut self, m: &mut Model<'_>, c: &Control) {
        for (prefix, channel) in [("memory", &self.sample), ("memory_meta", &self.metadata)] {
            if !m.has(&format!("{prefix}_valid")) {
                continue;
            }
            let response = channel.pending.is_some_and(|(delay, _, _, _)| delay == 0)
                && !c.pause
                && !c.pause_responses;
            let ready = channel.pending.is_none() && !c.pause && (!c.stalls || self.random.yes(4));
            for (suffix, value) in [
                ("ready", u64::from(ready)),
                ("response_valid", u64::from(response)),
                (
                    "response_error",
                    u64::from(response && channel.pending.unwrap().2),
                ),
                (
                    "read_data",
                    channel.pending.map_or(0, |(_, v, _, _)| u64::from(v)),
                ),
                ("write_ready", 0),
                ("read_valid", 0),
            ] {
                let name = format!("{prefix}_{suffix}");
                if m.has(&name) {
                    m.set(&name, value);
                }
            }
        }
    }
    pub fn sample(&mut self, m: &Model<'_>, c: &mut Control) {
        if m.get("reset") != 0 {
            self.sample = Channel::default();
            self.metadata = Channel::default();
            return;
        }
        for (prefix, channel, is_sample) in [
            ("memory_meta", &mut self.metadata, false),
            ("memory", &mut self.sample, true),
        ] {
            if !m.has(&format!("{prefix}_valid")) {
                continue;
            }
            let get = |suffix| m.get(&format!("{prefix}_{suffix}"));
            let packet = (get("write"), get("address"), get("write_data"));
            channel
                .held
                .check(prefix, get("valid") != 0, get("ready") != 0, packet);
            if get("response_valid") != 0 && get("response_ready") != 0 {
                channel.pending = None;
            } else if let Some((delay, _, _, _)) = &mut channel.pending {
                if *delay > 0 {
                    *delay -= 1;
                }
            }
            if get("valid") != 0 && get("ready") != 0 {
                assert!(channel.pending.is_none());
                let address = packet
                    .1
                    .checked_sub(self.base)
                    .expect("legacy address below arena");
                assert!(
                    address % 4 == 0 && address / 4 < self.words.len() as u64,
                    "legacy address outside arena"
                );
                let index = address as usize / 4;
                let error = std::mem::take(&mut c.fail_next);
                let write = packet.0 != 0;
                if write && !error {
                    self.words[index] = packet.2 as u32;
                }
                channel.pending = Some((
                    if c.stalls {
                        self.random.below(if is_sample { 8 } else { 4 })
                    } else {
                        0
                    },
                    self.words[index],
                    error,
                    write,
                ));
                if is_sample {
                    self.writes += u64::from(write);
                    self.reads += u64::from(!write);
                }
            }
        }
    }
}
