//! Bounded client-worker messages. The worker owns codec scratch and retained decoded input.
use crate::codec::{Decoded, Value, Work};
use anyhow::{anyhow, bail, ensure, Result};
use serde::{Deserialize, Serialize};
use shoop_app_api::IoTaskKind;
use shoop_app_protocol::{persistence::*, persistence_transfer::UploadStream, *};
use std::sync::Arc;

pub const MAX_RECORD: usize = 64 * 1024;
pub const FILE_CHUNK: usize = 16 * 1024;
pub const IMAGE_BATCH_PARTS: usize = 16;
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Summary {
    pub handle: u64,
    pub rate: u32,
    pub audio_labels: Option<Vec<String>>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Command {
    BeginFile {
        kind: u8,
        bytes: u64,
        rate: u32,
    },
    FileChunk {
        offset: u64,
        bytes: Vec<u8>,
    },
    FinishFile,
    BeginImage {
        description: Description,
        kind: u8,
        wav: bool,
        standard: bool,
        channels: Vec<u32>,
    },
    Image(Request),
    Resample {
        handle: u64,
        rate: u32,
    },
    Prepare {
        handle: u64,
        mapping: Vec<u32>,
        update_length: bool,
    },
    Next,
    NextImageBatch,
    Reset,
    ImageBatch(Vec<Request>),
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Response {
    Ack,
    Decoded(Summary),
    Image(Description),
    ImagePart(Request),
    ImageBatch {
        parts: Vec<Request>,
        done: bool,
    },
    File {
        name: String,
        mime: String,
        bytes: u64,
        warning: String,
    },
    FileChunk {
        offset: u64,
        bytes: Vec<u8>,
    },
    Done,
    Failed(String),
}
struct Writer(Vec<u8>);
impl bincode::enc::write::Writer for Writer {
    fn write(&mut self, bytes: &[u8]) -> std::result::Result<(), bincode::error::EncodeError> {
        let end = self
            .0
            .len()
            .checked_add(bytes.len())
            .filter(|end| *end <= MAX_RECORD)
            .ok_or(bincode::error::EncodeError::UnexpectedEnd)?;
        if end > self.0.capacity() {
            let capacity = end.max(self.0.capacity().max(1024) * 2).min(MAX_RECORD);
            self.0
                .try_reserve_exact(capacity - self.0.len())
                .map_err(|_| {
                    bincode::error::EncodeError::Other("codec record allocation failed")
                })?;
        }
        self.0.extend_from_slice(bytes);
        Ok(())
    }
}
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut bytes = Writer(Vec::new());
    bincode::serde::encode_into_writer(value, &mut bytes, bincode::config::standard())?;
    Ok(bytes.0)
}
pub fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    ensure!(
        bytes.len() <= MAX_RECORD,
        "codec worker record exceeds its limit"
    );
    let (value, consumed) = bincode::serde::decode_from_slice(
        bytes,
        bincode::config::standard().with_limit::<MAX_RECORD>(),
    )?;
    ensure!(consumed == bytes.len(), "trailing codec worker bytes");
    Ok(value)
}
pub fn kind(value: IoTaskKind) -> Result<u8> {
    Ok(match value {
        IoTaskKind::SaveSession => 0,
        IoTaskKind::LoadSession => 1,
        IoTaskKind::ExportLoopAudio => 2,
        IoTaskKind::ImportLoopAudio => 3,
        IoTaskKind::ExportLoopMidi => 4,
        IoTaskKind::ImportLoopMidi => 5,
        _ => bail!("unsupported codec task"),
    })
}
fn task_kind(kind: u8) -> Result<IoTaskKind> {
    Ok(match kind {
        0 => IoTaskKind::SaveSession,
        1 => IoTaskKind::LoadSession,
        2 => IoTaskKind::ExportLoopAudio,
        3 => IoTaskKind::ImportLoopAudio,
        4 => IoTaskKind::ExportLoopMidi,
        5 => IoTaskKind::ImportLoopMidi,
        _ => bail!("unknown codec task"),
    })
}
fn authority() -> Authority {
    Authority {
        session: SessionId(1),
        client: ClientId(1),
        generation: ConnectionGeneration(1),
    }
}
fn key() -> Key {
    Key {
        transfer: TransferId(1),
        epoch: 1,
    }
}
enum Input {
    File {
        kind: IoTaskKind,
        total: u64,
        bytes: Vec<u8>,
        rate: u32,
    },
    Image {
        receiver: Receiver,
        kind: IoTaskKind,
        wav: bool,
        standard: bool,
        channels: Vec<u32>,
    },
}
#[derive(Default)]
pub struct Worker {
    input: Option<Input>,
    decoded: Option<(u64, Decoded)>,
    image: Option<UploadStream>,
    file: Option<(Arc<Vec<u8>>, usize)>,
    next: u64,
}
impl Worker {
    fn clear(&mut self) {
        self.input = None;
        self.decoded = None;
        self.image = None;
        self.file = None;
    }
    pub fn receive(&mut self, command: Command) -> Response {
        match self.apply(command) {
            Ok(value) => value,
            Err(error) => {
                self.clear();
                let mut message = error.to_string();
                if message.len() > 1024 {
                    let mut end = 1024;
                    while !message.is_char_boundary(end) {
                        end -= 1;
                    }
                    message.truncate(end);
                }
                Response::Failed(message)
            }
        }
    }
    fn decoded(&mut self, value: Value) -> Result<Response> {
        let Value::Decoded(decoded) = value else {
            bail!("codec returned an unexpected result")
        };
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| anyhow!("codec handle space exhausted"))?;
        let summary = Summary {
            handle: self.next,
            rate: decoded.rate(),
            audio_labels: decoded.audio_labels(),
        };
        self.decoded = Some((self.next, decoded));
        Ok(Response::Decoded(summary))
    }
    fn take_decoded(&mut self, handle: u64) -> Result<Decoded> {
        ensure!(
            self.decoded.as_ref().is_some_and(|(id, _)| *id == handle),
            "stale decoded codec handle"
        );
        Ok(self.decoded.take().unwrap().1)
    }
    fn apply(&mut self, command: Command) -> Result<Response> {
        match command {
            Command::ImageBatch(parts) => {
                ensure!(
                    !parts.is_empty() && parts.len() <= IMAGE_BATCH_PARTS,
                    "invalid codec image batch length"
                );
                ensure!(
                    parts[..parts.len() - 1]
                        .iter()
                        .all(|part| !matches!(part, Request::Finish { .. })),
                    "codec image finish must end its batch"
                );
                let mut response = Response::Ack;
                for part in parts {
                    response = self.apply(Command::Image(part))?;
                }
                Ok(response)
            }
            Command::NextImageBatch => {
                let mut parts = Vec::new();
                for _ in 0..IMAGE_BATCH_PARTS {
                    let Some(stream) = &mut self.image else {
                        return Ok(Response::ImageBatch { parts, done: true });
                    };
                    match stream
                        .next_request()
                        .map_err(|r| anyhow!("codec image read failed: {r:?}"))?
                    {
                        Some(request) => parts.push(request),
                        None => {
                            self.image = None;
                            return Ok(Response::ImageBatch { parts, done: true });
                        }
                    }
                }
                Ok(Response::ImageBatch { parts, done: false })
            }
            Command::Reset => {
                self.clear();
                Ok(Response::Ack)
            }
            Command::BeginFile {
                kind,
                bytes: total,
                rate,
            } => {
                self.clear();
                ensure!(
                    total > 0 && total <= MAX_SESSION_FILE_BYTES && rate > 0,
                    "invalid codec file declaration"
                );
                let kind = task_kind(kind)?;
                ensure!(
                    matches!(
                        kind,
                        IoTaskKind::LoadSession
                            | IoTaskKind::ImportLoopAudio
                            | IoTaskKind::ImportLoopMidi
                    ),
                    "invalid input codec task"
                );
                let mut bytes = Vec::new();
                bytes.try_reserve_exact(total as usize)?;
                self.input = Some(Input::File {
                    kind,
                    total,
                    bytes,
                    rate,
                });
                Ok(Response::Ack)
            }
            Command::FileChunk {
                offset,
                bytes: chunk,
            } => {
                let Some(Input::File { bytes, total, .. }) = &mut self.input else {
                    bail!("no codec file input")
                };
                ensure!(
                    !chunk.is_empty()
                        && chunk.len() <= FILE_CHUNK
                        && offset == bytes.len() as u64
                        && offset.saturating_add(chunk.len() as u64) <= *total,
                    "invalid codec file chunk"
                );
                bytes.extend_from_slice(&chunk);
                Ok(Response::Ack)
            }
            Command::FinishFile => {
                let Some(Input::File {
                    kind,
                    total,
                    bytes,
                    rate,
                }) = self.input.take()
                else {
                    bail!("no codec file input")
                };
                ensure!(bytes.len() as u64 == total, "incomplete codec file");
                let value = Work::Decode {
                    kind,
                    bytes: bytes.into(),
                    rate,
                }
                .run()?;
                self.decoded(value)
            }
            Command::Resample { handle, rate } => {
                let decoded = self.take_decoded(handle)?;
                let value = Work::Resample { decoded, rate }.run()?;
                self.decoded(value)
            }
            Command::Prepare {
                handle,
                mapping,
                update_length,
            } => {
                ensure!(mapping.len() <= 2, "too many destination channels");
                let decoded = self.take_decoded(handle)?;
                let Value::Image(image) = (Work::Prepare {
                    decoded,
                    mapping,
                    update_length,
                })
                .run()?
                else {
                    bail!("codec returned an unexpected result")
                };
                let stream = UploadStream::new(
                    image,
                    key(),
                    Target::Session {
                        revision: Revision(1),
                    },
                )
                .map_err(|r| anyhow!("invalid codec image: {r:?}"))?;
                let description = stream.description();
                self.image = Some(stream);
                Ok(Response::Image(description))
            }
            Command::BeginImage {
                description,
                kind,
                wav,
                standard,
                channels,
            } => {
                self.clear();
                ensure!(channels.len() <= 2, "too many export channels");
                let kind = task_kind(kind)?;
                ensure!(
                    matches!(
                        kind,
                        IoTaskKind::SaveSession
                            | IoTaskKind::ExportLoopAudio
                            | IoTaskKind::ExportLoopMidi
                    ),
                    "invalid output codec task"
                );
                let receiver = Receiver::new(authority(), description)
                    .map_err(|r| anyhow!("invalid codec image: {r:?}"))?;
                self.input = Some(Input::Image {
                    receiver,
                    kind,
                    wav,
                    standard,
                    channels,
                });
                Ok(Response::Ack)
            }
            Command::Image(request) => {
                let Some(Input::Image { receiver, .. }) = &mut self.input else {
                    bail!("no codec image input")
                };
                if let Request::Begin(description) = request {
                    ensure!(
                        receiver.description() == description,
                        "codec image declaration mismatch"
                    );
                    return Ok(Response::Ack);
                }
                receiver
                    .apply(authority(), request)
                    .map_err(|r| anyhow!("invalid codec image transfer: {r:?}"))?;
                if !receiver.ready() {
                    return Ok(Response::Ack);
                }
                let Some(Input::Image {
                    receiver,
                    kind,
                    wav,
                    standard,
                    channels,
                }) = self.input.take()
                else {
                    unreachable!()
                };
                let image = shoop_app_protocol::persistence_transfer::assemble(
                    receiver
                        .into_objects()
                        .map_err(|r| anyhow!("incomplete image: {r:?}"))?,
                )
                .map_err(|r| anyhow!("invalid image: {r:?}"))?;
                let Value::File {
                    name,
                    mime,
                    bytes,
                    warning,
                } = (Work::Encode {
                    image,
                    kind,
                    wav,
                    standard,
                    channels,
                })
                .run()?
                else {
                    bail!("codec returned an unexpected result")
                };
                let total = bytes.len() as u64;
                self.file = Some((bytes, 0));
                Ok(Response::File {
                    name,
                    mime,
                    bytes: total,
                    warning,
                })
            }
            Command::Next => {
                if let Some(stream) = &mut self.image {
                    return match stream
                        .next_request()
                        .map_err(|r| anyhow!("codec image read failed: {r:?}"))?
                    {
                        Some(request) => Ok(Response::ImagePart(request)),
                        None => {
                            self.image = None;
                            Ok(Response::Done)
                        }
                    };
                }
                if let Some((bytes, offset)) = &mut self.file {
                    if *offset == bytes.len() {
                        self.file = None;
                        return Ok(Response::Done);
                    }
                    let begin = *offset;
                    *offset = bytes.len().min(begin + FILE_CHUNK);
                    return Ok(Response::FileChunk {
                        offset: begin as u64,
                        bytes: bytes[begin..*offset].to_vec(),
                    });
                }
                Ok(Response::Done)
            }
        }
    }
}

#[cfg(all(target_arch = "wasm32", feature = "worker-exports"))]
#[allow(unsafe_code)] // Exported symbol names; the ABI never accepts a host pointer.
mod exports {
    use super::*;
    use std::cell::RefCell;
    #[derive(Default)]
    struct State {
        worker: Worker,
        input: Vec<u8>,
        output: Vec<u8>,
    }
    thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }
    #[no_mangle]
    pub extern "C" fn codec_input(length: usize) -> usize {
        if length == 0 || length > MAX_RECORD {
            return 0;
        }
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.input.resize(length, 0);
            state.input.as_mut_ptr() as usize
        })
    }
    #[no_mangle]
    pub extern "C" fn codec_run() -> usize {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            let response = match decode::<Command>(&state.input) {
                Ok(command) => state.worker.receive(command),
                Err(error) => Response::Failed(error.to_string()),
            };
            state.output = encode(&response).unwrap_or_else(|_| {
                encode(&Response::Failed(
                    "codec response exceeded its bound".into(),
                ))
                .unwrap()
            });
            state.output.len()
        })
    }
    #[no_mangle]
    pub extern "C" fn codec_output() -> usize {
        STATE.with(|state| state.borrow().output.as_ptr() as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;
    #[shoop_test]
    fn worker_refuses_quotas_order_and_stale_handles_without_retained_work() {
        let mut worker = Worker::default();
        assert!(matches!(
            worker.receive(Command::BeginFile {
                kind: 3,
                bytes: MAX_SESSION_FILE_BYTES + 1,
                rate: 1000
            }),
            Response::Failed(_)
        ));
        assert!(worker.input.is_none());
        assert!(matches!(
            worker.receive(Command::BeginFile {
                kind: 3,
                bytes: 8,
                rate: 1000
            }),
            Response::Ack
        ));
        assert!(matches!(
            worker.receive(Command::FileChunk {
                offset: 1,
                bytes: vec![0]
            }),
            Response::Failed(_)
        ));
        assert!(worker.input.is_none());
        assert!(matches!(
            worker.receive(Command::Prepare {
                handle: 100,
                mapping: vec![0],
                update_length: true
            }),
            Response::Failed(_)
        ));
        assert!(decode::<Command>(&vec![0; MAX_RECORD + 1]).is_err());
        let mut bytes = encode(&Command::Reset).unwrap();
        bytes.push(0);
        assert!(decode::<Command>(&bytes).is_err());
        assert!(matches!(
            worker.receive(Command::ImageBatch(Vec::new())),
            Response::Failed(_)
        ));
        assert!(matches!(
            worker.receive(Command::ImageBatch(vec![
                Request::Finish { key: key() };
                IMAGE_BATCH_PARTS + 1
            ])),
            Response::Failed(_)
        ));
    }

    #[shoop_test]
    fn bounded_image_batches_match_scalar_export_and_preserve_exact_audio() {
        let expected = (0..8192)
            .map(|frame| if frame % 2 == 0 { 0.25 } else { -0.5 })
            .collect::<Vec<_>>();
        let audio = shoop_session::LoopAudio {
            sample_rate: 1000,
            channels: vec![shoop_session::LoopAudioChannel {
                label: "Source".into(),
                role: "direct".into(),
                samples: expected.clone(),
            }],
        };
        let Value::Decoded(decoded) = (Work::Decode {
            kind: IoTaskKind::ImportLoopAudio,
            bytes: shoop_session::encode_float_wav(&audio).unwrap().into(),
            rate: 1000,
        })
        .run()
        .unwrap() else {
            panic!("decode failed")
        };
        let Value::Image(image) = (Work::Prepare {
            decoded,
            mapping: vec![0],
            update_length: true,
        })
        .run()
        .unwrap() else {
            panic!("prepare failed")
        };
        let persistence_image::Manifest::Update {
            sample_rate,
            frames,
            audio,
            ..
        } = &image.manifest
        else {
            panic!("expected imported audio")
        };
        let image = Arc::new(persistence_image::Image {
            manifest: persistence_image::Manifest::Loop {
                sample_rate: *sample_rate,
                content: persistence_image::Loop {
                    id: shoop_app_api::LoopId::from_raw(1),
                    name: "Captured audio".into(),
                    sync: false,
                    frames: frames.unwrap(),
                    gain: 1.,
                    balance: 0.,
                    audio: audio.iter().map(|update| update.content.clone()).collect(),
                    midi: Vec::new(),
                },
            },
            payloads: image.payloads.clone(),
        });
        let mut stream = UploadStream::new(
            image,
            key(),
            Target::Session {
                revision: Revision(1),
            },
        )
        .unwrap();
        let description = stream.description();
        let mut requests = Vec::new();
        while let Some(request) = stream.next_request().unwrap() {
            requests.push(request);
        }
        assert!(requests.len() > IMAGE_BATCH_PARTS);
        let mut files = Vec::new();
        for width in [1, IMAGE_BATCH_PARTS] {
            let mut worker = Worker::default();
            assert!(matches!(
                worker.receive(Command::BeginImage {
                    description,
                    kind: 2,
                    wav: true,
                    standard: false,
                    channels: vec![0],
                }),
                Response::Ack
            ));
            let mut declared = None;
            for parts in requests.chunks(width) {
                let command = if width == 1 {
                    Command::Image(parts[0].clone())
                } else {
                    Command::ImageBatch(parts.to_vec())
                };
                let encoded = encode(&command).unwrap();
                assert!(encoded.len() <= MAX_RECORD);
                match worker.receive(decode(&encoded).unwrap()) {
                    Response::Ack => {}
                    Response::File { bytes, .. } => declared = Some(bytes),
                    response => panic!("image transfer failed: {response:?}"),
                }
            }
            let mut file = Vec::new();
            loop {
                match worker.receive(Command::Next) {
                    Response::FileChunk { offset, bytes } => {
                        assert_eq!(offset, file.len() as u64);
                        file.extend_from_slice(&bytes);
                    }
                    Response::Done => break,
                    response => panic!("file transfer failed: {response:?}"),
                }
            }
            assert_eq!(Some(file.len() as u64), declared);
            assert_eq!(
                shoop_session::decode_wav(&file).unwrap().channels[0].samples,
                expected
            );
            files.push(file);
        }
        assert_eq!(files[0], files[1]);
    }
    #[shoop_test]
    fn worker_retains_decoded_audio_across_dialog_and_streams_the_complete_image() {
        let expected = vec![0., 0.25, -0.5, 0.75];
        let audio = shoop_session::LoopAudio {
            sample_rate: 1000,
            channels: vec![shoop_session::LoopAudioChannel {
                label: "Source".into(),
                role: "direct".into(),
                samples: expected.clone(),
            }],
        };
        let bytes = shoop_session::encode_float_wav(&audio).unwrap();
        let mut worker = Worker::default();
        assert!(matches!(
            worker.receive(Command::BeginFile {
                kind: 3,
                bytes: bytes.len() as u64,
                rate: 1000
            }),
            Response::Ack
        ));
        for (index, bytes) in bytes.chunks(FILE_CHUNK).enumerate() {
            let command: Command = decode(
                &encode(&Command::FileChunk {
                    offset: (index * FILE_CHUNK) as u64,
                    bytes: bytes.to_vec(),
                })
                .unwrap(),
            )
            .unwrap();
            assert!(matches!(worker.receive(command), Response::Ack));
        }
        let Response::Decoded(summary) = worker.receive(Command::FinishFile) else {
            panic!("decode failed")
        };
        assert_eq!(summary.rate, 1000);
        assert_eq!(summary.audio_labels.as_ref().unwrap().len(), 1);
        let Response::Image(description) = worker.receive(Command::Prepare {
            handle: summary.handle,
            mapping: vec![0, 0],
            update_length: true,
        }) else {
            panic!("prepare failed")
        };
        let mut receiver = Receiver::new(authority(), description).unwrap();
        loop {
            match worker.receive(Command::Next) {
                Response::ImagePart(Request::Begin(other)) => assert_eq!(other, description),
                Response::ImagePart(request) => {
                    receiver.apply(authority(), request).unwrap();
                }
                Response::Done => break,
                response => panic!("unexpected response: {response:?}"),
            }
        }
        let image =
            shoop_app_protocol::persistence_transfer::assemble(receiver.into_objects().unwrap())
                .unwrap();
        assert_eq!(image.payloads.len(), 2);
        for payload in &image.payloads {
            let persistence_image::Payload::Audio(samples) = payload else {
                panic!("wrong payload")
            };
            assert_eq!(samples.as_slice(), expected);
        }
        assert!(worker.decoded.is_none());
        assert!(worker.image.is_none());
        assert!(matches!(worker.receive(Command::Next), Response::Done));
    }
}
