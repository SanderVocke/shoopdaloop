//! Finite latest-value storage for track-only global MIDI controls.
pub const CONTROL_KEYS: usize = 16 * 122;
const EMPTY: u16 = u16::MAX;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Message {
    bytes: [u8; 3],
    len: u8,
}
impl Message {
    pub fn data(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingControls {
    values: [u16; CONTROL_KEYS],
    len: usize,
}
impl Default for PendingControls {
    fn default() -> Self {
        Self {
            values: [EMPTY; CONTROL_KEYS],
            len: 0,
        }
    }
}
impl PendingControls {
    pub fn supports(data: &[u8]) -> bool {
        key(data).is_some()
    }
    /// Returns whether an older deferred value was replaced.
    pub fn store(&mut self, data: &[u8]) -> Option<bool> {
        let (key, value) = key(data)?;
        let previous = self.values[key];
        if previous == EMPTY {
            self.len += 1;
        }
        self.values[key] = value;
        Some(previous != EMPTY)
    }
    pub fn clear(&mut self, data: &[u8]) -> bool {
        let Some((key, _)) = key(data) else {
            return false;
        };
        if self.values[key] == EMPTY {
            return false;
        }
        self.values[key] = EMPTY;
        self.len -= 1;
        true
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// Advances a caller-owned cursor, ordered by channel, CC, pressure and pitch.
    pub fn next(&self, cursor: &mut usize) -> Option<Message> {
        while *cursor < CONTROL_KEYS {
            let index = *cursor;
            *cursor += 1;
            let value = self.values[index];
            if value == EMPTY {
                continue;
            }
            let channel = (index / 122) as u8;
            return Some(match index % 122 {
                controller @ 0..=119 => Message {
                    bytes: [0xb0 | channel, controller as u8, value as u8],
                    len: 3,
                },
                120 => Message {
                    bytes: [0xd0 | channel, value as u8, 0],
                    len: 2,
                },
                _ => Message {
                    bytes: [
                        0xe0 | channel,
                        (value & 127) as u8,
                        ((value >> 7) & 127) as u8,
                    ],
                    len: 3,
                },
            });
        }
        None
    }
}
fn key(data: &[u8]) -> Option<(usize, u16)> {
    let &status = data.first()?;
    let channel = usize::from(status & 15) * 122;
    match status & 0xf0 {
        0xb0 if data.len() >= 3 && data[1] < 120 => {
            Some((channel + usize::from(data[1]), u16::from(data[2])))
        }
        0xd0 if data.len() >= 2 => Some((channel + 120, u16::from(data[1]))),
        0xe0 if data.len() >= 3 => Some((
            channel + 121,
            u16::from(data[1]) | (u16::from(data[2]) << 7),
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;
    #[shoop_test]
    fn all_supported_control_keys_keep_latest_values_and_drain_in_reference_order() {
        let mut pending = PendingControls::default();
        for channel in (0..16).rev() {
            assert_eq!(pending.store(&[0xe0 | channel, 1, 2]), Some(false));
            assert_eq!(pending.store(&[0xd0 | channel, 3]), Some(false));
            for controller in (0..120).rev() {
                assert_eq!(pending.store(&[0xb0 | channel, controller, 4]), Some(false));
                assert_eq!(pending.store(&[0xb0 | channel, controller, 5]), Some(true));
            }
        }
        assert_eq!(pending.len(), CONTROL_KEYS);
        let mut cursor = 0;
        for channel in 0..16 {
            for controller in 0..120 {
                let packet = pending.next(&mut cursor).unwrap();
                assert_eq!(packet.data(), &[0xb0 | channel, controller, 5]);
                assert!(pending.clear(packet.data()));
                assert!(!pending.clear(packet.data()));
            }
            let packet = pending.next(&mut cursor).unwrap();
            assert_eq!(packet.data(), &[0xd0 | channel, 3]);
            assert!(pending.clear(packet.data()));
            let packet = pending.next(&mut cursor).unwrap();
            assert_eq!(packet.data(), &[0xe0 | channel, 1, 2]);
            assert!(pending.clear(packet.data()));
        }
        assert!(pending.next(&mut cursor).is_none());
        assert!(pending.is_empty());
        for data in [
            &[][..],
            &[0x90, 60, 100],
            &[0x80, 60, 0],
            &[0xb0, 120, 0],
            &[0xb0, 123, 0],
            &[0xc0, 1],
            &[0xa0, 60, 1],
            &[0xf0, 1, 0xf7],
        ] {
            assert!(!PendingControls::supports(data));
            assert_eq!(pending.store(data), None);
        }
    }
}
