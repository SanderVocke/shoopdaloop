use super::*;
use js_sys::{Array, Object, Reflect, Uint8Array, WebAssembly};
use shoop_app_protocol::{
    persistence_image::Validation,
    persistence_transfer::{Assembly, UploadStream},
};
use shoop_persistence_codec::worker::{self, Command as WorkerCommand, Response as WorkerResponse};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
const WINDOW: usize = 16;
const MEDIA_BYTES_PER_POLL: usize = 256 * 1024;
fn js_error(value: JsValue) -> anyhow::Error {
    anyhow!("file codec worker: {value:?}")
}
struct Peer {
    worker: web_sys::Worker,
    url: String,
    ready: Rc<Cell<bool>>,
    active: Rc<Cell<u32>>,
    incoming: Rc<RefCell<VecDeque<Vec<u8>>>>,
    error: Rc<RefCell<Option<String>>>,
    _message: Closure<dyn FnMut(web_sys::MessageEvent)>,
    _error: Closure<dyn FnMut(web_sys::ErrorEvent)>,
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.worker.set_onmessage(None);
        self.worker.set_onerror(None);
        self.worker.terminate();
        let _ = web_sys::Url::revoke_object_url(&self.url);
    }
}
impl Peer {
    fn new() -> Result<Rc<Self>> {
        let options = web_sys::BlobPropertyBag::new();
        options.set_type("text/javascript");
        let blob = web_sys::Blob::new_with_str_sequence_and_options(
            &Array::of1(&include_str!("codec_worker.js").into()),
            &options,
        )
        .map_err(js_error)?;
        let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(js_error)?;
        let worker = match web_sys::Worker::new(&url) {
            Ok(worker) => worker,
            Err(error) => {
                let _ = web_sys::Url::revoke_object_url(&url);
                return Err(js_error(error));
            }
        };
        let ready = Rc::new(Cell::new(false));
        let active = Rc::new(Cell::new(0));
        let incoming = Rc::new(RefCell::new(VecDeque::new()));
        let error = Rc::new(RefCell::new(None));
        let message = {
            let ready = Rc::clone(&ready);
            let active = Rc::clone(&active);
            let incoming = Rc::clone(&incoming);
            let error = Rc::clone(&error);
            Closure::new(move |event: web_sys::MessageEvent| {
                let data = event.data();
                if Reflect::get(&data, &"ready".into())
                    .ok()
                    .and_then(|v| v.as_bool())
                    == Some(true)
                {
                    ready.set(true);
                    return;
                }
                if let Some(message) = Reflect::get(&data, &"error".into())
                    .ok()
                    .and_then(|v| v.as_string())
                {
                    *error.borrow_mut() = Some(message);
                    return;
                }
                if Reflect::get(&data, &"job".into())
                    .ok()
                    .and_then(|v| v.as_f64())
                    != Some(f64::from(active.get()))
                {
                    return;
                }
                let Ok(buffer) = Reflect::get(&data, &"bytes".into()) else {
                    *error.borrow_mut() = Some("codec response has no bytes".into());
                    return;
                };
                let bytes = Uint8Array::new(&buffer);
                let mut queue = incoming.borrow_mut();
                if bytes.length() as usize > worker::MAX_RECORD || queue.len() >= 32 {
                    *error.borrow_mut() = Some("codec response window overflow".into());
                    return;
                }
                queue.push_back(bytes.to_vec());
            })
        };
        let on_error = {
            let error = Rc::clone(&error);
            Closure::new(move |event: web_sys::ErrorEvent| {
                *error.borrow_mut() =
                    Some(format!("file codec worker failed: {}", event.message()));
            })
        };
        worker.set_onmessage(Some(message.as_ref().unchecked_ref()));
        worker.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        let peer = Rc::new(Self {
            worker,
            url,
            ready,
            active,
            incoming,
            error,
            _message: message,
            _error: on_error,
        });
        let window = web_sys::window().ok_or_else(|| anyhow!("browser window unavailable"))?;
        let embedded = Reflect::get(window.as_ref(), &"shoopEmbeddedAudioWorklet".into())
            .unwrap_or(JsValue::UNDEFINED);
        let bytes = Reflect::get(&embedded, &"codecWasmBytes".into()).unwrap_or(JsValue::UNDEFINED);
        if !bytes.is_undefined() && !bytes.is_null() {
            let bindings = Reflect::get(&embedded, &"codecBindingsUrl".into()).map_err(js_error)?;
            ensure!(
                bindings.as_string().is_some(),
                "embedded codec bindings are missing"
            );
            let worker = peer.worker.clone();
            let error = Rc::clone(&peer.error);
            wasm_bindgen_futures::spawn_local(async move {
                let result: Result<()> = async {
                    let module = JsFuture::from(WebAssembly::compile(&bytes))
                        .await
                        .map_err(js_error)?;
                    let message = Object::new();
                    Reflect::set(&message, &"init".into(), &true.into()).map_err(js_error)?;
                    Reflect::set(&message, &"module".into(), &module).map_err(js_error)?;
                    Reflect::set(&message, &"bindingsUrl".into(), &bindings).map_err(js_error)?;
                    worker.post_message(&message).map_err(js_error)?;
                    Ok(())
                }
                .await;
                if let Err(problem) = result {
                    *error.borrow_mut() = Some(problem.to_string());
                }
            });
        } else {
            #[cfg(test)]
            let module_url = option_env!("SHOOP_WASM_TEST_ASSET_BASE").map(|base| {
                format!(
                    "{}/shoop_persistence_codec_bg.wasm",
                    base.trim_end_matches('/')
                )
            });
            #[cfg(not(test))]
            let module_url: Option<String> = None;
            let module_url = match module_url {
                Some(url) => url,
                None => web_sys::Url::new_with_base(
                    "./generated/shoop_persistence_codec_bg.wasm",
                    &window.location().href().map_err(js_error)?,
                )
                .map_err(js_error)?
                .href(),
            };
            let message = Object::new();
            Reflect::set(&message, &"init".into(), &true.into()).map_err(js_error)?;
            let bindings_url = module_url.replace(
                "shoop_persistence_codec_bg.wasm",
                "shoop_persistence_codec.js",
            );
            Reflect::set(&message, &"url".into(), &module_url.into()).map_err(js_error)?;
            Reflect::set(&message, &"bindingsUrl".into(), &bindings_url.into())
                .map_err(js_error)?;
            peer.worker.post_message(&message).map_err(js_error)?;
        }
        Ok(peer)
    }
    fn post(&self, id: u32, command: &WorkerCommand) -> Result<()> {
        let bytes = worker::encode(command)?;
        let buffer = Uint8Array::from(bytes.as_slice()).buffer();
        let message = Object::new();
        Reflect::set(&message, &"job".into(), &id.into()).map_err(js_error)?;
        Reflect::set(&message, &"bytes".into(), &buffer).map_err(js_error)?;
        self.worker
            .post_message_with_transfer(&message, &Array::of1(&buffer))
            .map_err(js_error)
    }
}
enum Sending {
    File {
        bytes: Arc<Vec<u8>>,
        kind: u8,
        rate: u32,
        offset: usize,
        phase: u8,
    },
    Image {
        validation: Option<Validation>,
        family: Option<u64>,
        stream: Option<UploadStream>,
        kind: u8,
        wav: bool,
        standard: bool,
        channels: Vec<u32>,
        declared: bool,
    },
    One(Option<WorkerCommand>),
}
impl Sending {
    fn next(&mut self) -> Result<Option<WorkerCommand>> {
        match self {
            Self::File {
                bytes,
                kind,
                rate,
                offset,
                phase,
            } => Ok(match *phase {
                0 => {
                    *phase = 1;
                    Some(WorkerCommand::BeginFile {
                        kind: *kind,
                        bytes: bytes.len() as u64,
                        rate: *rate,
                    })
                }
                1 if *offset < bytes.len() => {
                    let start = *offset;
                    *offset = bytes.len().min(start + worker::FILE_CHUNK);
                    Some(WorkerCommand::FileChunk {
                        offset: start as u64,
                        bytes: bytes[start..*offset].to_vec(),
                    })
                }
                1 => {
                    *phase = 2;
                    Some(WorkerCommand::FinishFile)
                }
                _ => None,
            }),
            Self::Image {
                validation,
                family,
                stream,
                kind,
                wav,
                standard,
                channels,
                declared,
            } => {
                if let Some(progress) = validation {
                    if !progress
                        .step(MEDIA_BYTES_PER_POLL)
                        .map_err(|r| anyhow!("invalid captured media: {r:?}"))?
                    {
                        return Ok(None);
                    }
                    let validated = validation
                        .take()
                        .unwrap()
                        .finish()
                        .map_err(|r| anyhow!("incomplete media validation: {r:?}"))?;
                    *stream = Some(
                        UploadStream::from_validated(
                            validated,
                            transfer::Key {
                                transfer: TransferId(1),
                                epoch: 1,
                            },
                            transfer::Target::Session {
                                revision: Revision(1),
                            },
                        )
                        .map_err(|r| anyhow!("invalid captured image: {r:?}"))?,
                    );
                }
                let stream = stream.as_mut().unwrap();
                if !*declared {
                    *declared = true;
                    if let Some(handle) = family.take() {
                        return Ok(Some(WorkerCommand::BeginFamily {
                            description: stream.description(),
                            handle,
                        }));
                    }
                    return Ok(Some(WorkerCommand::BeginImage {
                        description: stream.description(),
                        kind: *kind,
                        wav: *wav,
                        standard: *standard,
                        channels: std::mem::take(channels),
                    }));
                }
                let mut parts = Vec::with_capacity(worker::IMAGE_BATCH_PARTS);
                for _ in 0..worker::IMAGE_BATCH_PARTS {
                    let Some(request) = stream
                        .next_request()
                        .map_err(|r| anyhow!("captured image transfer failed: {r:?}"))?
                    else {
                        break;
                    };
                    parts.push(request);
                }
                Ok((!parts.is_empty()).then_some(WorkerCommand::ImageBatch(parts)))
            }
            Self::One(command) => Ok(command.take()),
        }
    }
}
enum Output {
    None,
    Image(transfer::Receiver),
    Assembly(Assembly),
    File {
        name: String,
        mime: String,
        warning: String,
        total: u64,
        bytes: Vec<u8>,
    },
}
struct BrowserJob {
    peer: Rc<Peer>,
    id: u32,
    sending: Sending,
    inflight: usize,
    output: Output,
}
impl BrowserJob {
    fn image_part(&mut self, request: transfer::Request) -> Result<()> {
        let Output::Image(receiver) = &mut self.output else {
            bail!("unexpected codec image data")
        };
        if let transfer::Request::Begin(description) = request {
            ensure!(
                receiver.description() == description,
                "codec image declaration changed"
            );
            return Ok(());
        }
        receiver
            .apply(
                Authority {
                    lease: LeaseId(1),
                    client: ClientId(1),
                    generation: ConnectionGeneration(1),
                },
                request,
            )
            .map_err(|r| anyhow!("invalid codec image: {r:?}"))?;
        Ok(())
    }
    fn done(&mut self) -> Result<Option<Value>> {
        match std::mem::replace(&mut self.output, Output::None) {
            Output::Image(receiver) => {
                self.output = Output::Assembly(
                    Assembly::new(
                        receiver
                            .into_objects()
                            .map_err(|r| anyhow!("incomplete codec image: {r:?}"))?,
                    )
                    .map_err(|r| anyhow!("invalid codec image: {r:?}"))?,
                );
                Ok(None)
            }
            Output::File {
                name,
                mime,
                warning,
                total,
                bytes,
            } => {
                ensure!(bytes.len() as u64 == total, "incomplete codec file");
                Ok(Some(Value::File {
                    name,
                    mime,
                    bytes: Arc::new(bytes),
                    warning,
                }))
            }
            output => {
                self.output = output;
                Ok(None)
            }
        }
    }
    fn poll(&mut self) -> Result<Option<Value>> {
        if let Some(error) = self.peer.error.borrow_mut().take() {
            bail!(error);
        }
        if !self.peer.ready.get() {
            return Ok(None);
        }
        for _ in 0..WINDOW {
            let Some(bytes) = self.peer.incoming.borrow_mut().pop_front() else {
                break;
            };
            self.inflight = self
                .inflight
                .checked_sub(1)
                .ok_or_else(|| anyhow!("unexpected codec response"))?;
            match worker::decode::<WorkerResponse>(&bytes)? {
                WorkerResponse::Ack => {}
                WorkerResponse::Failed(message) => bail!(message),
                WorkerResponse::Decoded(summary) => {
                    if let Some(conversion_required) = summary.conversion_required {
                        return Ok(Some(Value::Inspected {
                            decoded: Decoded::Remote(summary),
                            conversion_required,
                        }));
                    }
                    return Ok(Some(Value::Decoded(Decoded::Remote(summary))));
                }
                WorkerResponse::Image(description) => {
                    self.output = Output::Image(
                        transfer::Receiver::new(
                            Authority {
                                lease: LeaseId(1),
                                client: ClientId(1),
                                generation: ConnectionGeneration(1),
                            },
                            description,
                        )
                        .map_err(|r| anyhow!("invalid codec image: {r:?}"))?,
                    );
                }
                WorkerResponse::ImagePart(request) => self.image_part(request)?,
                WorkerResponse::ImageBatch { parts, done } => {
                    ensure!(
                        parts.len() <= worker::IMAGE_BATCH_PARTS,
                        "codec batch exceeded its bound"
                    );
                    if matches!(self.output, Output::Image(_)) {
                        for request in parts {
                            self.image_part(request)?;
                        }
                    } else {
                        ensure!(parts.is_empty(), "unexpected codec image data");
                    }
                    if done {
                        if let Some(value) = self.done()? {
                            return Ok(Some(value));
                        }
                    }
                }
                WorkerResponse::File {
                    name,
                    mime,
                    bytes: total,
                    warning,
                } => {
                    ensure!(
                        total <= transfer::MAX_SESSION_FILE_BYTES,
                        "codec output exceeds the file limit"
                    );
                    let mut bytes = Vec::new();
                    bytes.try_reserve_exact(total as usize)?;
                    self.output = Output::File {
                        name,
                        mime,
                        warning,
                        total,
                        bytes,
                    };
                }
                WorkerResponse::FileChunk {
                    offset,
                    bytes: chunk,
                } => {
                    let Output::File { bytes, total, .. } = &mut self.output else {
                        bail!("unexpected codec file data")
                    };
                    ensure!(
                        offset == bytes.len() as u64
                            && chunk.len() <= worker::FILE_CHUNK
                            && offset.saturating_add(chunk.len() as u64) <= *total,
                        "invalid codec file chunk"
                    );
                    bytes.extend_from_slice(&chunk);
                }
                WorkerResponse::Done => {
                    if let Some(value) = self.done()? {
                        return Ok(Some(value));
                    }
                }
            }
        }
        if let Output::Assembly(assembly) = &mut self.output {
            return Ok(assembly
                .step(MEDIA_BYTES_PER_POLL)
                .map_err(|r| anyhow!("invalid codec media: {r:?}"))?
                .map(|image| Value::Image(image.into_image())));
        }
        while self.inflight < WINDOW {
            let command = match self.output {
                Output::Image(_) => Some(WorkerCommand::NextImageBatch),
                Output::File { .. } => Some(WorkerCommand::Next),
                _ => self.sending.next()?,
            };
            let Some(command) = command else { break };
            self.peer.post(self.id, &command)?;
            self.inflight += 1;
        }
        Ok(None)
    }
}
enum Mode {
    Synchronous(Option<Work>),
    Assembly(Assembly),
    Browser(Box<BrowserJob>),
}
pub(crate) struct CodecJob(Mode);
pub(crate) struct CodecExecutor {
    peer: Option<Rc<Peer>>,
}
impl CodecExecutor {
    pub fn new() -> Self {
        Self { peer: None }
    }
    pub fn reset(&mut self) {
        self.peer = None;
    }
    pub fn start(&mut self, work: Work) -> Result<CodecJob> {
        if let Work::Assemble(objects) = work {
            return Ok(CodecJob(Mode::Assembly(
                Assembly::new(objects).map_err(|r| anyhow!("invalid capture: {r:?}"))?,
            )));
        }
        if web_sys::window().is_none() {
            return Ok(CodecJob(Mode::Synchronous(Some(work))));
        }
        let sending = match work {
            Work::GenerateClick { request, rate } => {
                Sending::One(Some(WorkerCommand::GenerateClick { request, rate }))
            }
            Work::Decode { kind, bytes, rate } => Sending::File {
                bytes,
                kind: worker::kind(kind)?,
                rate,
                offset: 0,
                phase: 0,
            },
            Work::Encode {
                image,
                kind,
                wav,
                standard,
                channels,
            } => Sending::Image {
                family: None,
                validation: Some(
                    Validation::for_transfer(image)
                        .map_err(|r| anyhow!("invalid capture: {r:?}"))?,
                ),
                stream: None,
                kind: worker::kind(kind)?,
                wav,
                standard,
                channels,
                declared: false,
            },
            Work::ConvertFamily {
                decoded: Decoded::Remote(summary),
                base,
            } => {
                ensure!(self.peer.is_some(), "decoded codec worker was lost");
                Sending::Image {
                    validation: Some(
                        Validation::for_transfer(base)
                            .map_err(|reason| anyhow!("invalid family capture: {reason:?}"))?,
                    ),
                    family: Some(summary.handle),
                    stream: None,
                    kind: 0,
                    wav: false,
                    standard: false,
                    channels: Vec::new(),
                    declared: false,
                }
            }
            Work::Prepare {
                decoded: Decoded::Remote(summary),
                mapping,
                update_length,
            } => {
                ensure!(self.peer.is_some(), "decoded codec worker was lost");
                Sending::One(Some(WorkerCommand::Prepare {
                    handle: summary.handle,
                    mapping,
                    update_length,
                }))
            }
            Work::Inspect {
                decoded: Decoded::Remote(summary),
                target,
                mapping,
                update_length,
            } => {
                ensure!(self.peer.is_some(), "decoded codec worker was lost");
                Sending::One(Some(WorkerCommand::Inspect {
                    handle: summary.handle,
                    target,
                    mapping,
                    update_length,
                }))
            }
            Work::Convert {
                decoded: Decoded::Remote(summary),
                target,
                update_length,
            } => {
                ensure!(self.peer.is_some(), "decoded codec worker was lost");
                Sending::One(Some(WorkerCommand::Convert {
                    handle: summary.handle,
                    target,
                    update_length,
                }))
            }
            _ => bail!("decoded media is not owned by the browser codec worker"),
        };
        if self.peer.is_none() {
            self.peer = Some(Peer::new()?);
        }
        let peer = Rc::clone(self.peer.as_ref().unwrap());
        let id = peer
            .active
            .get()
            .checked_add(1)
            .ok_or_else(|| anyhow!("codec job identity exhausted"))?;
        peer.active.set(id);
        peer.incoming.borrow_mut().clear();
        Ok(CodecJob(Mode::Browser(Box::new(BrowserJob {
            peer,
            id,
            sending,
            inflight: 0,
            output: Output::None,
        }))))
    }
}
impl CodecJob {
    pub fn poll(&mut self) -> Option<Result<Value, String>> {
        let result = match &mut self.0 {
            Mode::Synchronous(work) => {
                return work
                    .take()
                    .map(|work| work.run().map_err(|e| e.to_string()))
            }
            Mode::Assembly(assembly) => assembly
                .step(MEDIA_BYTES_PER_POLL)
                .map(|value| value.map(|image| Value::Captured(image.into_image())))
                .map_err(|r| anyhow!("invalid captured media: {r:?}")),
            Mode::Browser(job) => job.poll(),
        };
        match result {
            Ok(Some(value)) => Some(Ok(value)),
            Ok(None) => None,
            Err(error) => Some(Err(error.to_string())),
        }
    }
}
