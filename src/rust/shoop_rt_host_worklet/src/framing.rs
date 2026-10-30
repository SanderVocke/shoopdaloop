//! Protocol records over `postMessage`.
//!
//! A batch is the bytes of whole records, back to back. Every record starts with
//! its little-endian `u16` total length (see `shoop_rt_protocol::codec`), so
//! batches split without further framing, and a byte stream can be cut anywhere
//! and reassembled.

use shoop_rt_protocol::MAX_RECORD_BYTES;

/// A length prefix outside the codec's bounds: the stream cannot be split any
/// further.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FramingError;

/// Length of the first record in `bytes`, or `None` while it is incomplete.
pub fn record_len(bytes: &[u8]) -> Result<Option<usize>, FramingError> {
    let &[low, high, ..] = bytes else {
        return Ok(None);
    };
    let len = usize::from(u16::from_le_bytes([low, high]));
    if !(2..=MAX_RECORD_BYTES).contains(&len) {
        return Err(FramingError);
    }
    Ok((bytes.len() >= len).then_some(len))
}

/// The records of a batch of whole records. An incomplete last record is a
/// framing error.
pub fn records(mut batch: &[u8]) -> impl Iterator<Item = Result<&[u8], FramingError>> {
    std::iter::from_fn(move || {
        if batch.is_empty() {
            return None;
        }
        match record_len(batch) {
            Ok(Some(len)) => {
                let (record, rest) = batch.split_at(len);
                batch = rest;
                Some(Ok(record))
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
    fn splits_whole_records_and_waits_for_incomplete_ones() {
        assert_eq!(record_len(&[]), Ok(None));
        assert_eq!(record_len(&[5]), Ok(None));
        assert_eq!(record_len(&[5, 0, 0, 1]), Ok(None));
        assert_eq!(record_len(&[5, 0, 0, 1, 0, 9]), Ok(Some(5)));
        let batch = [5, 0, 0, 1, 0, 3, 0, 4];
        assert_eq!(
            records(&batch).collect::<Vec<_>>(),
            [Ok(&batch[..5]), Ok(&batch[5..])]
        );
    }

    #[shoop_test]
    fn rejects_lengths_outside_the_codec_bounds() {
        assert_eq!(record_len(&[0, 0]), Err(FramingError));
        assert_eq!(record_len(&[1, 0, 7]), Err(FramingError));
        let too_long = (MAX_RECORD_BYTES as u16 + 1).to_le_bytes();
        assert_eq!(record_len(&too_long), Err(FramingError));
        assert_eq!(
            records(&[3, 0, 4, 9, 0]).collect::<Vec<_>>(),
            [Ok(&[3, 0, 4][..]), Err(FramingError)]
        );
    }
}
