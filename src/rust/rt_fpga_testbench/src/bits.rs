//! Little-endian packed RTL words, including values wider than a native integer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bits(pub Vec<u32>);
impl Bits {
    pub fn zero(width: usize) -> Self {
        Self(vec![0; width.div_ceil(32)])
    }
    pub fn from_u64(value: u64) -> Self {
        Self(vec![value as u32, (value >> 32) as u32])
    }
    pub fn get(&self, offset: usize, width: usize) -> u64 {
        assert!(width <= 64);
        (0..width).fold(0, |v, i| {
            v | (u64::from(
                (self.0.get((offset + i) / 32).copied().unwrap_or(0) >> ((offset + i) % 32)) & 1,
            ) << i)
        })
    }
    pub fn set(&mut self, offset: usize, width: usize, value: u64) -> &mut Self {
        assert!(width <= 64);
        self.0
            .resize(self.0.len().max((offset + width).div_ceil(32)), 0);
        for i in 0..width {
            let bit = offset + i;
            self.0[bit / 32] = (self.0[bit / 32] & !(1 << (bit % 32)))
                | ((((value >> i) & 1) as u32) << (bit % 32));
        }
        self
    }
    pub fn fits(&self, width: usize) -> bool {
        self.0.iter().enumerate().all(|(i, &v)| {
            let remaining = width.saturating_sub(i * 32);
            remaining >= 32 || (remaining != 0 && v >> remaining == 0) || v == 0
        })
    }
    pub fn resized(&self, width: usize) -> Self {
        assert!(
            self.fits(width),
            "packed value does not fit {width} bits: {self:?}"
        );
        let mut value = self.clone();
        value.0.resize(width.div_ceil(32), 0);
        value
    }
    pub fn array<const N: usize>(&self) -> [u32; N] {
        std::array::from_fn(|i| self.0.get(i).copied().unwrap_or(0))
    }
}
impl<const N: usize> From<[u32; N]> for Bits {
    fn from(value: [u32; N]) -> Self {
        Self(value.to_vec())
    }
}
