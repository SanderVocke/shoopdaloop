//! Construction of staged image/configuration operations; no device time is predicted.
use crate::protocol::put;
use rt_runtime::Error;

pub struct Prepared {
    pub dimensions: Dimensions,
    pub uploads: Vec<Transfer>,
    pub activation: Transfer,
}
/// Prepare immutable writes and a copy map; retained values are read by the FPGA at activation.
pub fn prepare(
    old: &crate::image::Image,
    new: &crate::image::Image,
    copies: &[(u16, u16)],
    nonce: u64,
) -> Result<Prepared, Error> {
    let instructions =
        u32::try_from(new.program.instructions.len()).map_err(|_| Error::Capacity)?;
    let states = u32::try_from(new.initial_state.memory.len()).map_err(|_| Error::Capacity)?;
    if instructions > 65536 || states > 65536 || copies.len() > states as usize {
        return Err(Error::Capacity);
    }
    let mut seen = std::collections::BTreeSet::new();
    for &(source, destination) in copies {
        let from = old
            .initial_state
            .memory
            .get(usize::from(source))
            .ok_or(Error::InvalidId)?;
        let to = new
            .initial_state
            .memory
            .get(usize::from(destination))
            .ok_or(Error::InvalidId)?;
        if !seen.insert(destination) || std::mem::discriminant(from) != std::mem::discriminant(to) {
            return Err(Error::InvalidState);
        }
    }
    let mut uploads = Vec::with_capacity(instructions as usize + states as usize + copies.len());
    for (i, &instruction) in new.program.instructions.iter().enumerate() {
        uploads.push(Transfer::upload(
            false,
            i as u16,
            crate::image_wire::instruction_words(instruction)?,
            nonce,
        ));
    }
    for (i, &state) in new.initial_state.memory.iter().enumerate() {
        uploads.push(Transfer::upload(
            true,
            i as u16,
            crate::image_wire::value_words(state),
            nonce,
        ));
    }
    for (i, &(source, destination)) in copies.iter().enumerate() {
        let ramp = matches!(
            old.initial_state.memory[usize::from(source)],
            crate::image::Value::Ramp(_)
        );
        uploads.push(Transfer::copy(i as u16, source, destination, ramp, nonce));
    }
    Ok(Prepared {
        dimensions: Dimensions {
            instructions,
            states,
            jobs: new.program.jobs_per_quantum,
            buffers: new.buffers,
            copies: copies.len() as u32,
        },
        uploads,
        activation: Transfer::activate(nonce),
    })
}

#[derive(Clone, Copy)]
pub struct Dimensions {
    pub instructions: u32,
    pub states: u32,
    pub jobs: u32,
    pub buffers: u32,
    pub copies: u32,
}
#[derive(Clone)]
pub struct Transfer {
    pub opcode: u8,
    pub state: bool,
    pub address: u16,
    pub words: [u32; 20],
}
impl Transfer {
    pub fn packet(&self, sequence: u64) -> [u32; 24] {
        let mut words = [0; 24];
        words[..20].copy_from_slice(&self.words);
        put(&mut words, 640, 16, u64::from(self.address));
        put(&mut words, 671, 1, u64::from(self.state));
        put(&mut words, 672, 8, u64::from(self.opcode));
        put(&mut words, 680, 2, 1);
        put(&mut words, 682, 64, sequence);
        words
    }
    fn new(opcode: u8, state: bool, address: u16) -> Self {
        Self {
            opcode,
            state,
            address,
            words: [0; 20],
        }
    }
    pub fn begin(version: u64, dimensions: Dimensions) -> Self {
        let mut v = Self::new(1, false, 0);
        put(&mut v.words, 0, 64, version);
        for (offset, n) in [
            (64, dimensions.instructions),
            (96, dimensions.states),
            (128, dimensions.jobs),
            (160, dimensions.buffers),
            (192, dimensions.copies),
        ] {
            put(&mut v.words, offset, 32, u64::from(n));
        }
        v
    }
    pub fn begin_bank(bank: u16, version: u64, dimensions: Dimensions) -> Self {
        let mut t = Self::begin(version, dimensions);
        t.address = bank;
        t
    }
    pub fn status_bank(bank: u16) -> Self {
        Self::new(7, false, bank)
    }
    pub fn activate_canonical(nonce: u64) -> Self {
        let mut t = Self::activate(nonce);
        t.state = true;
        t
    }
    pub fn patch_staged(address: u16, words: [u32; 5], ramp: bool, nonce: u64) -> Self {
        let mut t = Self::new(9, ramp, address);
        t.words[..5].copy_from_slice(&words);
        put(&mut t.words, 129, 64, nonce);
        t
    }
    pub fn read_staged(address: u16, ramp: bool, nonce: u64) -> Self {
        let mut t = Self::new(10, ramp, address);
        put(&mut t.words, 129, 64, nonce);
        t
    }
    pub fn evaluate_ramp(words: [u32; 5], target: i32, duration: u32) -> Self {
        let mut t = Self::new(11, true, 0);
        t.words[..5].copy_from_slice(&words);
        put(&mut t.words, 129, 32, u64::from(target as u32));
        put(&mut t.words, 161, 32, u64::from(duration));
        t
    }
    pub fn upload(state: bool, address: u16, words: [u32; 5], nonce: u64) -> Self {
        let mut v = Self::new(if state { 3 } else { 2 }, state, address);
        v.words[..5].copy_from_slice(&words);
        put(&mut v.words, 129, 64, nonce);
        v
    }
    pub fn copy(index: u16, source: u16, destination: u16, ramp: bool, nonce: u64) -> Self {
        let mut v = Self::new(4, false, index);
        put(&mut v.words, 0, 16, u64::from(source));
        put(&mut v.words, 16, 16, u64::from(destination));
        put(&mut v.words, 32, 1, u64::from(ramp));
        put(&mut v.words, 129, 64, nonce);
        v
    }
    pub fn activate(nonce: u64) -> Self {
        let mut v = Self::new(5, false, 0);
        put(&mut v.words, 0, 64, nonce);
        v
    }
    pub fn set_ramp(address: u16, target: i32, duration: u32, version: u64) -> Self {
        let mut v = Self::new(6, true, address);
        put(&mut v.words, 0, 32, u64::from(target as u32));
        put(&mut v.words, 32, 32, u64::from(duration));
        put(&mut v.words, 129, 64, version);
        v
    }
    pub fn read(state: bool, address: u16) -> Self {
        Self::new(8, state, address)
    }
    pub fn status() -> Self {
        Self::new(7, false, 0)
    }
    pub fn write(state: bool, address: u16, words: [u32; 5]) -> Self {
        let mut v = Self::new(0, state, address);
        v.words[..5].copy_from_slice(&words);
        v
    }
}
