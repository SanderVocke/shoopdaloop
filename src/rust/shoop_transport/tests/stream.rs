use core::time::Duration;
use shoop_transport::{
    handshake::{Error, Handshake},
    stream::{ByteIo, Config, IoError, Stream},
    CarrierError, CodecError, Lane, RecordPort,
};
use shoop_wasm_test_support::shoop_test;

struct Scripted {
    input: [u8; 256],
    read: usize,
    end: usize,
    output: [u8; 256],
    written: usize,
    fragment: usize,
    blocked: bool,
    eof: bool,
    interrupt: bool,
}
impl Scripted {
    fn new(bytes: &[u8]) -> Self {
        let mut input = [0; 256];
        input[..bytes.len()].copy_from_slice(bytes);
        Self {
            input,
            read: 0,
            end: bytes.len(),
            output: [0; 256],
            written: 0,
            fragment: 1,
            blocked: false,
            eof: false,
            interrupt: false,
        }
    }
}
impl ByteIo for Scripted {
    fn try_read(&mut self, out: &mut [u8]) -> Result<usize, IoError> {
        if self.interrupt {
            self.interrupt = false;
            return Err(IoError::Interrupted);
        }
        if self.read == self.end {
            return if self.eof {
                Ok(0)
            } else {
                Err(IoError::WouldBlock)
            };
        }
        let n = out.len().min(self.fragment).min(self.end - self.read);
        out[..n].copy_from_slice(&self.input[self.read..self.read + n]);
        self.read += n;
        Ok(n)
    }
    fn try_write(&mut self, bytes: &[u8]) -> Result<usize, IoError> {
        if self.blocked {
            return Err(IoError::WouldBlock);
        }
        let n = bytes.len().min(self.fragment);
        self.output[self.written..self.written + n].copy_from_slice(&bytes[..n]);
        self.written += n;
        Ok(n)
    }
}

#[shoop_test]
fn fragmented_stream_preserves_frames_and_terminal_failures() {
    let mut stream = Stream::new(Scripted::new(&[3, 0, 7, 4, 0, 8, 9]), Config::default()).unwrap();
    let mut out = [0; 1280];
    let mut records = Vec::new();
    for _ in 0..16 {
        if let Some(n) = stream.take(&mut out) {
            records.push(out[..n].to_vec());
        }
    }
    assert_eq!(records, [vec![3, 0, 7], vec![4, 0, 8, 9]]);
    assert_eq!(stream.failure(), None);
    stream.io_mut().eof = true;
    stream.take(&mut out);
    assert_eq!(stream.failure(), Some(CarrierError::Disconnected));
    let mut broken = Stream::new(Scripted::new(&[1, 0]), Config::default()).unwrap();
    for _ in 0..3 {
        broken.take(&mut out);
    }
    assert_eq!(
        broken.failure(),
        Some(CarrierError::Codec(CodecError::Invalid))
    );
    let mut truncated = Stream::new(Scripted::new(&[4, 0, 1]), Config::default()).unwrap();
    truncated.io_mut().eof = true;
    for _ in 0..3 {
        truncated.take(&mut out);
    }
    assert_eq!(
        truncated.failure(),
        Some(CarrierError::Codec(CodecError::Truncated))
    );
}

#[shoop_test]
fn bounded_queues_retry_without_losing_or_growing_records() {
    let mut io = Scripted::new(&[]);
    io.blocked = true;
    let mut stream = Stream::new(
        io,
        Config {
            reliable: 1,
            bulk: 1,
            telemetry: 1,
        },
    )
    .unwrap();
    stream.post(&[3, 0, 11]).unwrap();
    assert_eq!(stream.post(&[3, 0, 12]), Err(CarrierError::Full));
    stream.post_on_lane(&[3, 0, 21], Lane::Telemetry).unwrap();
    stream.post_on_lane(&[3, 0, 22], Lane::Telemetry).unwrap();
    assert_eq!(stream.pending(), 2);
    stream.io_mut().blocked = false;
    stream.flush().unwrap();
    assert_eq!(&stream.io().output[..6], &[3, 0, 11, 3, 0, 22]);
    assert!(stream.outgoing_empty());
}

#[shoop_test]
fn compatibility_handshake_handles_partial_io_and_remaining_deadlines() {
    use shoop_transport::handshake::{Intent, MAGIC};
    let mut request = MAGIC.to_vec();
    request.push(1);
    let mut server = Handshake::new(true, Duration::from_secs(1));
    let mut io = Scripted::new(&request);
    io.interrupt = true;
    for _ in 0..16 {
        if server.advance(&mut io, Duration::from_millis(1)).unwrap() {
            break;
        }
    }
    assert!(server.advance(&mut io, Duration::ZERO).unwrap());
    assert_eq!(server.intent(), Intent::Inspect);
    assert_eq!(&io.output[..io.written], &MAGIC);
    let mut client = Handshake::inspect(Duration::from_secs(1));
    let mut io = Scripted::new(&MAGIC);
    for _ in 0..16 {
        if client.advance(&mut io, Duration::from_millis(1)).unwrap() {
            break;
        }
    }
    assert!(client.advance(&mut io, Duration::ZERO).unwrap());
    assert_eq!(&io.output[..io.written], &request);
    let mut timeout = Handshake::new(true, Duration::from_millis(10));
    assert_eq!(
        timeout.advance(&mut Scripted::new(&[]), Duration::from_millis(10)),
        Err(Error::Timeout)
    );
    let mut wrong = Handshake::new(true, Duration::from_secs(1));
    let mut io = Scripted::new(&[0; 9]);
    io.fragment = 9;
    assert_eq!(wrong.advance(&mut io, Duration::ZERO), Err(Error::Protocol));
    assert_eq!(wrong.advance(&mut io, Duration::ZERO), Err(Error::Protocol));
}

#[cfg(all(not(target_arch = "wasm32"), debug_assertions))]
#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

#[shoop_test(no_wasm = "native allocation guard")]
fn stream_progression_is_heap_free_after_preparation() {
    let mut stream = Stream::new(Scripted::new(&[3, 0, 7]), Config::default()).unwrap();
    let mut output = [0; 1280];
    assert_no_alloc::assert_no_alloc(|| {
        stream.post(&[3, 0, 8]).unwrap();
        stream.flush().unwrap();
        for _ in 0..4 {
            stream.take(&mut output);
        }
    });
}
