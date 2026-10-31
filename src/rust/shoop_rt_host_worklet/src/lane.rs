//! The MIDI lanes: timestamped MIDI events between the main thread and the
//! worklet, apart from protocol records. Web MIDI input travels into the engine's
//! host MIDI inputs; the engine's host MIDI outputs travel back for Web MIDI
//! output.
//!
//! An event is a little-endian `u64` frame on the AudioWorklet's `currentFrame`
//! timeline, a `u8` host MIDI port index, a `u8` message length from 1 to 255 and
//! the message bytes. A batch is events back to back.

use crate::framing::FramingError;

pub const EVENT_HEADER_BYTES: usize = 10;
pub const MAX_EVENT_MIDI_BYTES: usize = u8::MAX as usize;
pub const MAX_EVENT_BYTES: usize = EVENT_HEADER_BYTES + MAX_EVENT_MIDI_BYTES;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiEvent<'a> {
    pub frame: u64,
    pub port: u8,
    pub bytes: &'a [u8],
}

impl MidiEvent<'_> {
    /// Write the event to the start of `out` and return its length; `None` if it
    /// does not fit or its message length is out of range.
    pub fn encode(&self, out: &mut [u8]) -> Option<usize> {
        let len = EVENT_HEADER_BYTES + self.bytes.len();
        if self.bytes.is_empty() || self.bytes.len() > MAX_EVENT_MIDI_BYTES || out.len() < len {
            return None;
        }
        out[..8].copy_from_slice(&self.frame.to_le_bytes());
        out[8] = self.port;
        out[9] = self.bytes.len() as u8;
        out[EVENT_HEADER_BYTES..len].copy_from_slice(self.bytes);
        Some(len)
    }
}

/// The first event in `bytes` and its encoded length, or `None` while it is
/// incomplete.
pub fn decode(bytes: &[u8]) -> Result<Option<(MidiEvent<'_>, usize)>, FramingError> {
    let Some(header) = bytes.get(..EVENT_HEADER_BYTES) else {
        return Ok(None);
    };
    let midi = usize::from(header[9]);
    if midi == 0 {
        return Err(FramingError);
    }
    let len = EVENT_HEADER_BYTES + midi;
    let Some(message) = bytes.get(EVENT_HEADER_BYTES..len) else {
        return Ok(None);
    };
    let frame = u64::from_le_bytes(header[..8].try_into().expect("eight bytes"));
    Ok(Some((
        MidiEvent {
            frame,
            port: header[8],
            bytes: message,
        },
        len,
    )))
}

/// The events of a batch of whole events. An incomplete last event is a framing
/// error.
pub fn events(mut batch: &[u8]) -> impl Iterator<Item = Result<MidiEvent<'_>, FramingError>> {
    std::iter::from_fn(move || {
        if batch.is_empty() {
            return None;
        }
        match decode(batch) {
            Ok(Some((event, len))) => {
                batch = &batch[len..];
                Some(Ok(event))
            }
            Ok(None) | Err(FramingError) => {
                batch = &[];
                Some(Err(FramingError))
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test]
    fn events_round_trip_through_a_batch() {
        let first = MidiEvent {
            frame: (1 << 40) | 7,
            port: 2,
            bytes: &[0x90, 60, 100],
        };
        let second = MidiEvent {
            frame: 8,
            port: 0,
            bytes: &[0xf8],
        };
        let mut batch = [0; 32];
        let a = first.encode(&mut batch).unwrap();
        let b = second.encode(&mut batch[a..]).unwrap();
        assert_eq!((a, b), (13, 11));
        assert_eq!(
            events(&batch[..a + b]).collect::<Vec<_>>(),
            [Ok(first), Ok(second)]
        );
        assert_eq!(decode(&batch[..a - 1]), Ok(None));
        assert_eq!(events(&batch[..a + b - 1]).last(), Some(Err(FramingError)));
    }

    #[shoop_test]
    fn refuses_empty_oversized_and_unfitting_messages() {
        let mut out = [0; MAX_EVENT_BYTES + 1];
        let event = |bytes| MidiEvent {
            frame: 0,
            port: 0,
            bytes,
        };
        assert_eq!(event(&[]).encode(&mut out), None);
        assert_eq!(event(&[0; MAX_EVENT_MIDI_BYTES + 1]).encode(&mut out), None);
        assert_eq!(
            event(&[0; MAX_EVENT_MIDI_BYTES]).encode(&mut out),
            Some(MAX_EVENT_BYTES)
        );
        assert_eq!(event(&[0x90, 1, 2]).encode(&mut out[..12]), None);
        assert_eq!(decode(&[0; EVENT_HEADER_BYTES]), Err(FramingError));
    }
}
