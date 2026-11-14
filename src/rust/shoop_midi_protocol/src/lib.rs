//! Complete short-message validation without allocation or runtime dependencies.
#![no_std]
#![forbid(unsafe_code)]

pub const MAX_MESSAGE_BYTES: usize = 4;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Rejection {
    TooLong = 0,
    SysEx = 1,
    Parameter = 2,
    UnsupportedStatus = 3,
    Malformed = 4,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Disposition {
    Managed,
    LiveOnly,
}
pub fn parameter_controller(cc: u8) -> bool {
    matches!(cc, 6 | 38 | 96..=101)
}
pub fn classify(data: &[u8], live_only_parameters: bool) -> Result<Disposition, Rejection> {
    if data.len() > MAX_MESSAGE_BYTES {
        return Err(Rejection::TooLong);
    }
    let status = *data.first().ok_or(Rejection::Malformed)?;
    if matches!(status, 0xf0 | 0xf7) {
        return Err(Rejection::SysEx);
    }
    let expected = match status {
        0x80..=0xbf | 0xe0..=0xef => 3,
        0xc0..=0xdf => 2,
        0xff => 1,
        _ => return Err(Rejection::UnsupportedStatus),
    };
    if data.len() != expected || data[1..].iter().any(|&byte| byte >= 128) {
        return Err(Rejection::Malformed);
    }
    if status & 0xf0 == 0xb0 && parameter_controller(data[1]) {
        return if live_only_parameters {
            Ok(Disposition::LiveOnly)
        } else {
            Err(Rejection::Parameter)
        };
    }
    Ok(Disposition::Managed)
}
