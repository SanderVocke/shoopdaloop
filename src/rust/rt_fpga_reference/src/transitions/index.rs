use alloc::{vec, vec::Vec};

pub(super) struct SlotMask {
    levels: Vec<Vec<u64>>,
}

impl SlotMask {
    pub fn new(slots: usize) -> Self {
        let mut levels = Vec::new();
        let mut words = slots.div_ceil(64).max(1);
        loop {
            levels.push(vec![0; words]);
            if words == 1 {
                break;
            }
            words = words.div_ceil(64);
        }
        Self { levels }
    }
    pub fn clear(&mut self) {
        for level in &mut self.levels {
            level.fill(0);
        }
    }
    pub fn copy_from(&mut self, other: &Self) {
        for (level, source) in self.levels.iter_mut().zip(&other.levels) {
            level.copy_from_slice(source);
        }
    }
    pub fn insert(&mut self, mut slot: usize) {
        for level in &mut self.levels {
            let word = slot / 64;
            let previous = level[word];
            level[word] |= 1 << (slot % 64);
            if previous != 0 {
                break;
            }
            slot = word;
        }
    }
    pub fn remove(&mut self, mut slot: usize) {
        for level in &mut self.levels {
            let word = slot / 64;
            level[word] &= !(1 << (slot % 64));
            if level[word] != 0 {
                break;
            }
            slot = word;
        }
    }
    pub fn pop_first(&mut self) -> Option<usize> {
        let mut slot = 0;
        for level in self.levels.iter().rev() {
            let word = level[slot];
            if word == 0 {
                return None;
            }
            slot = 64 * slot + word.trailing_zeros() as usize;
        }
        self.remove(slot);
        Some(slot)
    }
}

const NONE: u32 = u32::MAX;

/// Slot-ordered intrusive lists retain stable record addresses and linear storage.
pub(super) struct PendingRequests {
    heads: Vec<[u32; 2]>,
    next: Vec<u32>,
    previous: Vec<u32>,
    pub absolute: SlotMask,
}
impl PendingRequests {
    pub fn new(loops: usize, requests: usize) -> Self {
        Self {
            heads: vec![[NONE; 2]; loops],
            next: vec![NONE; requests],
            previous: vec![NONE; requests],
            absolute: SlotMask::new(requests),
        }
    }
    pub fn copy_from(&mut self, other: &Self) {
        self.heads.copy_from_slice(&other.heads);
        self.next.copy_from_slice(&other.next);
        self.previous.copy_from_slice(&other.previous);
        self.absolute.copy_from(&other.absolute);
    }
    pub fn first(&self, target: usize, absolute: bool) -> Option<usize> {
        index(self.heads[target][usize::from(absolute)])
    }
    pub fn next(&self, slot: usize) -> Option<usize> {
        index(self.next[slot])
    }
    pub fn insert(&mut self, target: usize, slot: usize, absolute: bool) {
        let mut previous = NONE;
        let head = &mut self.heads[target][usize::from(absolute)];
        let mut next = *head;
        while next != NONE && next < slot as u32 {
            previous = next;
            next = self.next[next as usize];
        }
        self.previous[slot] = previous;
        self.next[slot] = next;
        if previous == NONE {
            *head = slot as u32;
        } else {
            self.next[previous as usize] = slot as u32;
        }
        if next != NONE {
            self.previous[next as usize] = slot as u32;
        }
        if absolute {
            self.absolute.insert(slot);
        }
    }
    pub fn remove(&mut self, target: usize, slot: usize, absolute: bool) {
        let previous = self.previous[slot];
        let next = self.next[slot];
        if previous == NONE {
            self.heads[target][usize::from(absolute)] = next;
        } else {
            self.next[previous as usize] = next;
        }
        if next != NONE {
            self.previous[next as usize] = previous;
        }
        // Keep the successor available to a walk already visiting this slot.
        if absolute {
            self.absolute.remove(slot);
        }
    }
}

fn index(slot: u32) -> Option<usize> {
    (slot != NONE).then_some(slot as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeSet;
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test]
    fn priority_mask_preserves_order_across_word_and_summary_boundaries() {
        let mut mask = SlotMask::new(65536);
        let mut expected = BTreeSet::new();
        for slot in [65535, 4096, 63, 64, 0, 4095, 65, 4097, 32768, 65534] {
            mask.insert(slot);
            mask.insert(slot);
            expected.insert(slot);
        }
        for slot in [64, 4095, 0, 65535, 64] {
            mask.remove(slot);
            expected.remove(&slot);
        }
        let mut copy = SlotMask::new(65536);
        copy.copy_from(&mask);
        mask.clear();
        assert_eq!(mask.pop_first(), None);
        while let Some(slot) = expected.pop_first() {
            assert_eq!(copy.pop_first(), Some(slot));
        }
        assert_eq!(copy.pop_first(), None);
        copy.insert(65535);
        assert_eq!(copy.pop_first(), Some(65535));
        assert_eq!(copy.pop_first(), None);
    }
}
