use shoop_transport::{
    stream::{ByteIo, Config, IoError, Stream},
    Lane, RecordPort,
};
use shoop_wasm_test_support::shoop_test;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
#[derive(Default)]
struct Io {
    blocked: bool,
    bytes: Vec<u8>,
    budget: Option<usize>,
}
impl ByteIo for Io {
    fn try_read(&mut self, _: &mut [u8]) -> Result<usize, IoError> {
        Err(IoError::WouldBlock)
    }
    fn try_write(&mut self, bytes: &[u8]) -> Result<usize, IoError> {
        if self.blocked {
            return Err(IoError::WouldBlock);
        }
        let len = self
            .budget
            .map_or(bytes.len(), |budget| budget.min(bytes.len()));
        if len == 0 {
            return Err(IoError::WouldBlock);
        }
        self.bytes.extend_from_slice(&bytes[..len]);
        if let Some(budget) = &mut self.budget {
            *budget -= len;
        }
        Ok(len)
    }
}
#[shoop_test]
fn saturated_bulk_and_telemetry_do_not_starve_profiling_or_delay_reliable_priority() {
    let mut stream = Stream::new(
        Io {
            blocked: true,
            ..Default::default()
        },
        Config {
            reliable: 2,
            bulk: 2,
            telemetry: 2,
        },
    )
    .unwrap();
    for (tag, lane) in [
        (1, Lane::Bulk),
        (2, Lane::Bulk),
        (3, Lane::Telemetry),
        (4, Lane::Telemetry),
        (5, Lane::Profiling),
        (6, Lane::Reliable),
    ] {
        stream.post_on_lane(&[3, 0, tag], lane).unwrap();
    }
    stream.io_mut().blocked = false;
    stream.flush().unwrap();
    let order: Vec<_> = stream.io().bytes.chunks_exact(3).map(|r| r[2]).collect();
    assert_eq!(order[0], 6);
    assert!(
        order.iter().position(|t| *t == 5).unwrap() < order.iter().position(|t| *t == 2).unwrap()
    );
    assert!(stream.outgoing_empty());
}

#[shoop_test]
fn a_started_record_finishes_before_reliable_traffic_without_interleaving_bytes() {
    let mut stream = Stream::new(
        Io {
            budget: Some(1),
            ..Default::default()
        },
        Config {
            reliable: 2,
            bulk: 2,
            telemetry: 2,
        },
    )
    .unwrap();
    stream.post_on_lane(&[3, 0, 1], Lane::Bulk).unwrap();
    stream.post_on_lane(&[3, 0, 2], Lane::Reliable).unwrap();
    stream.io_mut().budget = None;
    stream.flush().unwrap();
    assert_eq!(stream.io().bytes, [3, 0, 1, 3, 0, 2]);
}
