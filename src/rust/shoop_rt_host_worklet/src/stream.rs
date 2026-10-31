//! Byte buffers in module memory that the processor script fills and empties.

use crate::framing::FramingError;

/// An inbound byte stream. The script appends at [`Stream::len`] and commits; the
/// host consumes from the front and compacts once per quantum, so appended bytes
/// may split records or events anywhere.
pub(crate) struct Stream {
    bytes: Box<[u8]>,
    start: usize,
    end: usize,
}

impl Stream {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            bytes: vec![0; capacity].into_boxed_slice(),
            start: 0,
            end: 0,
        }
    }
    #[cfg(feature = "exports")]
    pub(crate) fn as_mut_ptr(&mut self) -> *mut u8 {
        self.bytes.as_mut_ptr()
    }
    #[cfg(feature = "exports")]
    pub(crate) fn capacity(&self) -> usize {
        self.bytes.len()
    }
    /// Offset of the next appended byte.
    #[cfg(any(test, feature = "exports"))]
    pub(crate) fn len(&self) -> usize {
        self.end
    }
    pub(crate) fn free_mut(&mut self) -> &mut [u8] {
        &mut self.bytes[self.end..]
    }
    /// Accept `count` bytes written at [`Stream::len`].
    pub(crate) fn commit(&mut self, count: usize) -> bool {
        if count > self.bytes.len() - self.end {
            return false;
        }
        self.end += count;
        true
    }
    pub(crate) fn pending(&self) -> &[u8] {
        &self.bytes[self.start..self.end]
    }
    pub(crate) fn consume(&mut self, count: usize) {
        self.start = (self.start + count).min(self.end);
    }
    pub(crate) fn clear(&mut self) {
        self.start = 0;
        self.end = 0;
    }
    pub(crate) fn compact(&mut self) {
        self.bytes.copy_within(self.start..self.end, 0);
        self.end -= self.start;
        self.start = 0;
    }

    /// Visit the complete items at the front, keep those that `visit` asks to
    /// keep, and compact. `visit` sees the bytes from an item's start and returns
    /// its length and whether to keep it, or `None` when it is incomplete. A
    /// framing error drops everything from the failing item on.
    pub(crate) fn retain(
        &mut self,
        mut visit: impl FnMut(&[u8]) -> Result<Option<(usize, bool)>, FramingError>,
    ) -> Result<(), FramingError> {
        let (mut read, mut write) = (self.start, 0);
        let result = loop {
            match visit(&self.bytes[read..self.end]) {
                Ok(Some((len, keep))) => {
                    if keep {
                        self.bytes.copy_within(read..read + len, write);
                        write += len;
                    }
                    read += len;
                }
                Ok(None) => break Ok(()),
                Err(error) => {
                    read = self.end;
                    break Err(error);
                }
            }
        };
        self.bytes.copy_within(read..self.end, write);
        self.end = write + (self.end - read);
        self.start = 0;
        result
    }
}

/// An outbound batch that the host fills during one quantum and the script posts
/// after it.
pub(crate) struct Batch {
    bytes: Box<[u8]>,
    len: usize,
}

impl Batch {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            bytes: vec![0; capacity].into_boxed_slice(),
            len: 0,
        }
    }
    #[cfg(feature = "exports")]
    pub(crate) fn as_ptr(&self) -> *const u8 {
        self.bytes.as_ptr()
    }
    #[cfg(feature = "exports")]
    pub(crate) fn capacity(&self) -> usize {
        self.bytes.len()
    }
    pub(crate) fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
    pub(crate) fn clear(&mut self) {
        self.len = 0;
    }
    pub(crate) fn push(&mut self, data: &[u8]) -> bool {
        self.write(|free| {
            let target = free.get_mut(..data.len())?;
            target.copy_from_slice(data);
            Some(data.len())
        })
    }
    /// Append what `encode` writes to the start of the free space, if it fits.
    pub(crate) fn write(&mut self, encode: impl FnOnce(&mut [u8]) -> Option<usize>) -> bool {
        match encode(&mut self.bytes[self.len..]) {
            Some(count) => {
                self.len += count;
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::heap::assert_no_heap;
    use shoop_wasm_test_support::shoop_test;

    fn append(stream: &mut Stream, bytes: &[u8]) {
        let free = stream.free_mut();
        free[..bytes.len()].copy_from_slice(bytes);
        assert!(stream.commit(bytes.len()));
    }

    #[shoop_test]
    fn stream_appends_consumes_and_compacts_without_allocating() {
        let mut stream = Stream::new(8);
        assert_no_heap(|| {
            append(&mut stream, &[1, 2, 3, 4, 5]);
            assert!(!stream.commit(4));
            stream.consume(2);
            assert_eq!(stream.pending(), [3, 4, 5]);
            stream.compact();
            assert_eq!((stream.pending(), stream.len()), (&[3, 4, 5][..], 3));
            append(&mut stream, &[6, 7, 8, 9, 10]);
            assert_eq!(stream.free_mut().len(), 0);
        });
    }

    #[shoop_test]
    fn retain_keeps_selected_items_in_order_and_the_incomplete_tail() {
        // Items are a length byte and payload; odd payloads are kept.
        let mut stream = Stream::new(16);
        append(&mut stream, &[2, 1, 2, 2, 3, 3, 3, 5]);
        let visit = |bytes: &[u8]| -> Result<Option<(usize, bool)>, FramingError> {
            let Some(&len) = bytes.first() else {
                return Ok(None);
            };
            if len == 0 {
                return Err(FramingError);
            }
            let len = usize::from(len);
            Ok((bytes.len() >= len).then(|| (len, bytes[1] % 2 == 1)))
        };
        assert_no_heap(|| stream.retain(visit)).unwrap();
        assert_eq!(stream.pending(), [2, 1, 3, 3, 3, 5]);
        append(&mut stream, &[7, 7, 7, 7, 0, 9]);
        assert_eq!(stream.retain(visit), Err(FramingError));
        assert_eq!(stream.pending(), [2, 1, 3, 3, 3, 5, 7, 7, 7, 7]);
    }

    #[shoop_test]
    fn batch_refuses_what_does_not_fit() {
        let mut batch = Batch::new(4);
        assert_no_heap(|| {
            assert!(batch.push(&[1, 2, 3]));
            assert!(!batch.push(&[4, 5]));
            assert!(batch.push(&[4]));
        });
        assert_eq!(batch.as_slice(), [1, 2, 3, 4]);
        batch.clear();
        assert!(batch.as_slice().is_empty());
    }
}
