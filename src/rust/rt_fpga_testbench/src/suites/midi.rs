use crate::{clock::Edges, model::Model, random::Random, sim::Agent};
pub mod data;

pub struct Memory {
    pins: [&'static str; 9],
    pub words: Vec<u32>,
    rng: Random,
    pending: Option<(u64, u32, bool)>,
    pub fail: Option<usize>,
    pub transfers: u64,
}
impl Memory {
    pub fn new(words: usize) -> Self {
        Self {
            pins: [
                "memory_ready",
                "memory_response_valid",
                "memory_read_data",
                "memory_response_error",
                "memory_response_ready",
                "memory_valid",
                "memory_address",
                "memory_write",
                "memory_write_data",
            ],
            words: vec![0xdeadbeef; words],
            rng: Random::new(1192016),
            pending: None,
            fail: None,
            transfers: 0,
        }
    }
}
impl Memory {
    pub fn mixed(words: usize) -> Self {
        let mut memory = Self::new(words);
        memory.pins = [
            "midi_memory_ready",
            "midi_memory_response_valid",
            "midi_memory_read_data",
            "midi_memory_response_error",
            "midi_memory_response_ready",
            "midi_memory_valid",
            "midi_memory_address",
            "midi_memory_write",
            "midi_memory_write_data",
        ];
        memory
    }
}
impl Agent for Memory {
    fn drive(&mut self, m: &mut Model<'_>, e: Edges) {
        if !e.rises(0) {
            return;
        }
        m.set(
            self.pins[0],
            u64::from(self.pending.is_none() && self.rng.yes(3)),
        );
        m.set(
            self.pins[1],
            u64::from(self.pending.is_some_and(|p| p.0 == 0)),
        );
        m.set(self.pins[2], self.pending.map_or(0, |p| u64::from(p.1)));
        m.set(self.pins[3], u64::from(self.pending.is_some_and(|p| p.2)));
    }
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if !e.rises(0) {
            return;
        }
        if m.get("reset") != 0 {
            self.pending = None;
            return;
        }
        if m.get(self.pins[1]) != 0 && m.get(self.pins[4]) != 0 {
            self.pending = None;
        } else if let Some(p) = &mut self.pending {
            p.0 = p.0.saturating_sub(1);
        }
        if m.get(self.pins[5]) != 0 && m.get(self.pins[0]) != 0 {
            let address = m.get(self.pins[6]);
            assert_eq!(address % 4, 0);
            let index = address as usize / 4;
            assert!(index < self.words.len(), "DDR word {index} out of range");
            let error = self.fail == Some(index);
            if m.get(self.pins[7]) != 0 && !error {
                self.words[index] = m.get(self.pins[8]) as u32;
            }
            self.transfers += 1;
            self.pending = Some((self.rng.below(4), self.words[index], error));
        }
    }
}
