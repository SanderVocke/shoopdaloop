//! Stable seeded stimulus, independent of library RNG version changes.
pub struct Random(u64);
impl Random {
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    pub fn below(&mut self, end: u64) -> u64 {
        assert!(end > 0);
        self.next() % end
    }
    pub fn yes(&mut self, denominator: u64) -> bool {
        self.below(denominator) != 0
    }
    pub fn signed(&mut self) -> i32 {
        self.next() as i32
    }
}
