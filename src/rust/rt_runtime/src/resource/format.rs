use crate::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Format {
    Audio { channels: u32 },
    Midi,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Description {
    pub format: Format,
    pub frames: u64,
    pub bytes: usize,
}
impl Description {
    pub(crate) fn validate(self, capacity: usize) -> Result<(), Error> {
        if self.bytes > capacity {
            return Err(Error::Capacity);
        }
        match self.format {
            Format::Audio { channels } => {
                if channels == 0 {
                    return Err(Error::InvalidShape);
                }
                let expected = self
                    .frames
                    .checked_mul(u64::from(channels))
                    .and_then(|n| n.checked_mul(4))
                    .ok_or(Error::Capacity)?;
                if usize::try_from(expected).ok() != Some(self.bytes) {
                    return Err(Error::InvalidShape);
                }
            }
            Format::Midi if self.frames == 0 && self.bytes != 0 => return Err(Error::InvalidFrame),
            Format::Midi => {}
        }
        Ok(())
    }
}

/// Incremental parser for u64-frame/u32-length MIDI records. No message-sized
/// scratch or final whole-message rescan is required, including for SysEx.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Parser {
    header: [u8; 12],
    header_len: usize,
    message_len: usize,
    received: usize,
    status: u8,
    last_frame: u64,
}
impl Parser {
    pub fn complete(self) -> bool {
        self.header_len == 0 && self.message_len == 0
    }
    pub fn consume(&mut self, bytes: &[u8], frames: u64, max_packet: usize) -> Result<(), Error> {
        for &byte in bytes {
            if self.message_len == 0 {
                self.header[self.header_len] = byte;
                self.header_len += 1;
                if self.header_len == 12 {
                    let frame = u64::from_le_bytes(
                        self.header[..8]
                            .try_into()
                            .map_err(|_| Error::InvalidState)?,
                    );
                    let len = u32::from_le_bytes(
                        self.header[8..]
                            .try_into()
                            .map_err(|_| Error::InvalidState)?,
                    ) as usize;
                    if frame < self.last_frame || frame >= frames {
                        return Err(Error::InvalidFrame);
                    }
                    if len == 0 || len > max_packet {
                        return Err(Error::Capacity);
                    }
                    self.last_frame = frame;
                    self.message_len = len;
                    self.received = 0;
                    self.header_len = 0;
                }
            } else {
                if self.received == 0 {
                    self.status = byte;
                    let expected = match byte {
                        0x80..=0xbf | 0xe0..=0xef | 0xf2 => 3,
                        0xc0..=0xdf | 0xf1 | 0xf3 => 2,
                        0xf6 | 0xf8 | 0xfa..=0xfc | 0xfe | 0xff => 1,
                        0xf0 if self.message_len >= 2 => self.message_len,
                        _ => return Err(Error::InvalidEvent),
                    };
                    if self.message_len != expected {
                        return Err(Error::InvalidEvent);
                    }
                } else if self.status == 0xf0 && self.received + 1 == self.message_len {
                    if byte != 0xf7 {
                        return Err(Error::InvalidEvent);
                    }
                } else if byte >= 0x80 {
                    return Err(Error::InvalidEvent);
                }
                self.received += 1;
                if self.received == self.message_len {
                    self.message_len = 0;
                }
            }
        }
        Ok(())
    }
}

/// Control-side convenience encoding. The caller supplies output storage.
pub fn encode_midi_record(frame: u64, data: &[u8], output: &mut [u8]) -> Result<usize, Error> {
    let len = u32::try_from(data.len()).map_err(|_| Error::Capacity)?;
    let total = data.len().checked_add(12).ok_or(Error::Capacity)?;
    if total > output.len() {
        return Err(Error::Capacity);
    }
    crate::midi::Message::parse(data, data.len())?;
    output[..8].copy_from_slice(&frame.to_le_bytes());
    output[8..12].copy_from_slice(&len.to_le_bytes());
    output[12..total].copy_from_slice(data);
    Ok(total)
}
