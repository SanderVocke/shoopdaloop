//! Compact action and result encoding at the device boundary.
use crate::protocol::{get, put};
use rt_runtime::Error;

pub const ABI: u32 = 3;
pub const PUBLICATION: usize = 429;
pub const TARGET: usize = 45;
pub const SEQUENCE: usize = 109;
pub const AT: usize = 173;
pub const SOURCE: usize = 237;
pub const SKIP: usize = 301;
pub const POSITION: usize = 365;
pub const RESULT_TARGET: usize = 45;
pub const RESULT_SEQUENCE: usize = 77;
pub const RESULT_POSITION: usize = 141;
pub const EFFECTIVE: usize = 205;
pub const REMAINING: usize = 269;
pub const FRAME: usize = 333;

pub fn install(original: &[u32; 20], generation: u32) -> [u32; 20] {
    let mut words = [0; 20];
    put(&mut words, 1, 32, u64::from(generation));
    put(&mut words, 33, 4, get(original, 33, 4));
    put(&mut words, 44, 1, get(original, 44, 1));
    for (source, target) in [
        (64, TARGET),
        (128, SEQUENCE),
        (192, AT),
        (256, SOURCE),
        (320, SKIP),
        (384, POSITION),
    ] {
        put(&mut words, target, 64, get(original, source, 64));
    }
    words
}

pub fn result(
    words: &[u32; 26],
    original: Option<&[u32; 20]>,
    generation: u32,
) -> Result<[u32; 26], Error> {
    if get(words, 1, 32) != u64::from(generation)
        || get(words, 397, 19) != 0
        || words[13..].iter().any(|&word| word != 0)
    {
        return Err(Error::Disconnected);
    }
    let mut value = [0; 26];
    if get(words, 0, 1) == 0 {
        let mut empty = [0; 26];
        put(&mut empty, 1, 32, u64::from(generation));
        return if original.is_none() && words == &empty {
            Ok(empty)
        } else {
            Err(Error::Disconnected)
        };
    }
    let original = original.ok_or(Error::Disconnected)?;
    if get(words, 33, 4) != get(original, 33, 4)
        || get(words, 44, 1) != get(original, 44, 1)
        || get(words, RESULT_TARGET, 32) != get(original, 64, 32)
        || get(words, RESULT_SEQUENCE, 64) != get(original, 128, 64)
        || get(words, RESULT_POSITION, 64) != get(original, 384, 64)
        || get(words, 37, 3) > 4
        || get(words, 40, 4) > 10
    {
        return Err(Error::Disconnected);
    }
    value[..20].copy_from_slice(original);
    value[0] = 0;
    value[1] = 0;
    put(&mut value, 0, 45, get(words, 0, 45));
    for (source, target) in [(EFFECTIVE, 448), (REMAINING, 512), (FRAME, 576)] {
        put(&mut value, target, 64, get(words, source, 64));
    }
    Ok(value)
}
