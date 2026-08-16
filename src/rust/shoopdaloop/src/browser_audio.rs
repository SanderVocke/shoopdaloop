use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use js_sys::{Array, Object, Reflect, WebAssembly};
use shoop_audio_protocol::{COMMAND_MAX_BYTES, MAX_DEVICE_AUDIO_CHANNELS, PROTOCOL_VERSION};
use shoop_backend::BackendDriverState;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AudioContext, AudioContextState, AudioWorkletNode, AudioWorkletNodeOptions, Event as WebEvent,
    HtmlButtonElement, MediaStream, MediaStreamAudioSourceNode, MediaStreamConstraints,
    MediaStreamTrack, MessageEvent, MessagePort, Response,
};
use web_time::Instant;

const WORKLET_NAME: &str = "shoop-audio-processor";
const WORKLET_SCRIPT_URL: &str = "./audio_worklet.js";
const WORKLET_WASM_URL: &str = "./generated/shoop_audio_worklet.wasm";
const EMBEDDED_WORKLET_ASSETS: &str = "shoopEmbeddedAudioWorklet";
const MAX_QUANTUM: u32 = 2048;

#[derive(Clone, Copy, PartialEq, Eq)]
enum AudioInputMode {
    Microphone,
    OutputOnly,
}

struct BrowserMessageEndpoint(MessagePort);

impl shoop_worklet_client::MessageEndpoint for BrowserMessageEndpoint {
    fn post_message(&self, message: &str) -> Result<()> {
        self.0
            .post_message(&JsValue::from_str(message))
            .map_err(|error| anyhow!("could not post worklet command: {error:?}"))
    }

    fn close(&self) {
        self.0.close();
    }
}

struct PhysicalAudioDriverState {
    generation: u64,
    startup_started: Option<Instant>,
    transport: shoop_worklet_client::RemoteBackendControl,
    context: Option<AudioContext>,
    stream: Option<MediaStream>,
    source: Option<MediaStreamAudioSourceNode>,
    node: Option<AudioWorkletNode>,
    message_handler: Option<Closure<dyn FnMut(MessageEvent)>>,
    processor_error_handler: Option<Closure<dyn FnMut(WebEvent)>>,
    context_state_handler: Option<Closure<dyn FnMut(WebEvent)>>,
    track_ended_handlers: Vec<Closure<dyn FnMut(WebEvent)>>,
    input_mode: Option<AudioInputMode>,
    repaint_context: Option<egui::Context>,
}

/// Owns browser audio resources and the restricted remote transport control.
struct BrowserAudioDriver {
    state: Rc<RefCell<PhysicalAudioDriverState>>,
}

/// Owns only DOM presentation callbacks and narrow packaged diagnostics.
struct BrowserAudioPresentation {
    microphone_enable_handler: Closure<dyn FnMut(WebEvent)>,
    output_enable_handler: Closure<dyn FnMut(WebEvent)>,
    suspend_handler: Closure<dyn FnMut(WebEvent)>,
    resume_handler: Closure<dyn FnMut(WebEvent)>,
    fail_handler: Closure<dyn FnMut(WebEvent)>,
    track_end_handler: Closure<dyn FnMut(WebEvent)>,
    shutdown_handler: Closure<dyn FnMut(WebEvent)>,
}

/// Composes the independently owned physical driver and presentation adapter.
pub struct BrowserAudioController {
    driver: BrowserAudioDriver,
    presentation: BrowserAudioPresentation,
}

impl BrowserAudioController {
    pub fn new(transport: shoop_worklet_client::RemoteBackendControl) -> Result<Self> {
        let window = web_sys::window().ok_or_else(|| anyhow!("browser window is unavailable"))?;
        let inner = Rc::new(RefCell::new(PhysicalAudioDriverState {
            generation: 0,
            startup_started: None,
            transport,
            context: None,
            stream: None,
            source: None,
            node: None,
            message_handler: None,
            processor_error_handler: None,
            context_state_handler: None,
            track_ended_handlers: Vec::new(),
            input_mode: None,
            repaint_context: None,
        }));
        let weak = Rc::downgrade(&inner);
        let microphone_enable_handler = Closure::wrap(Box::new(move |_event: WebEvent| {
            if let Some(inner) = weak.upgrade() {
                begin_enable(inner, AudioInputMode::Microphone);
            }
        }) as Box<dyn FnMut(_)>);
        microphone_enable_button()?
            .set_onclick(Some(microphone_enable_handler.as_ref().unchecked_ref()));

        let weak = Rc::downgrade(&inner);
        let output_enable_handler = Closure::wrap(Box::new(move |_event: WebEvent| {
            if let Some(inner) = weak.upgrade() {
                begin_enable(inner, AudioInputMode::OutputOnly);
            }
        }) as Box<dyn FnMut(_)>);
        output_enable_button()?.set_onclick(Some(output_enable_handler.as_ref().unchecked_ref()));

        let weak = Rc::downgrade(&inner);
        let suspend_handler = Closure::wrap(Box::new(move |_event: WebEvent| {
            if let Some(inner) = weak.upgrade() {
                if let Some(context) = &inner.borrow().context {
                    let _ = context.suspend();
                }
            }
        }) as Box<dyn FnMut(_)>);
        let weak = Rc::downgrade(&inner);
        let resume_handler = Closure::wrap(Box::new(move |_event: WebEvent| {
            if let Some(inner) = weak.upgrade() {
                if let Some(context) = &inner.borrow().context {
                    let _ = context.resume();
                }
            }
        }) as Box<dyn FnMut(_)>);
        let weak = Rc::downgrade(&inner);
        let fail_handler = Closure::wrap(Box::new(move |_event: WebEvent| {
            if let Some(inner) = weak.upgrade() {
                let control = inner.borrow().transport.clone();
                control.detach(true);
                control.fail("AudioWorklet processor stopped");
            }
        }) as Box<dyn FnMut(_)>);
        let weak = Rc::downgrade(&inner);
        let track_end_handler = Closure::wrap(Box::new(move |_event: WebEvent| {
            let stream = weak
                .upgrade()
                .and_then(|inner| inner.borrow().stream.clone());
            let Some(value) = stream.and_then(|stream| stream.get_audio_tracks().iter().next())
            else {
                return;
            };
            if let Ok(track) = value.dyn_into::<MediaStreamTrack>() {
                track.stop();
                if let Ok(event) = WebEvent::new("ended") {
                    let _ = track.dispatch_event(&event);
                }
            }
        }) as Box<dyn FnMut(_)>);
        let weak = Rc::downgrade(&inner);
        let shutdown_handler = Closure::wrap(Box::new(move |_event: WebEvent| {
            if let Some(inner) = weak.upgrade() {
                shutdown_inner(&mut inner.borrow_mut());
            }
        }) as Box<dyn FnMut(_)>);
        let diagnostics = Object::new();
        for (name, handler) in [
            ("suspend", &suspend_handler),
            ("resume", &resume_handler),
            ("fail", &fail_handler),
            ("endTrack", &track_end_handler),
            ("shutdown", &shutdown_handler),
        ] {
            Reflect::set(&diagnostics, &name.into(), handler.as_ref()).map_err(|error| {
                anyhow!("could not install physical audio diagnostic {name}: {error:?}")
            })?;
        }
        Reflect::set(
            window.as_ref(),
            &"shoopAudioDiagnostics".into(),
            &diagnostics,
        )
        .map_err(|error| anyhow!("could not publish physical audio diagnostics: {error:?}"))?;
        Ok(Self {
            driver: BrowserAudioDriver { state: inner },
            presentation: BrowserAudioPresentation {
                microphone_enable_handler,
                output_enable_handler,
                suspend_handler,
                resume_handler,
                fail_handler,
                track_end_handler,
                shutdown_handler,
            },
        })
    }

    pub fn state(&self) -> BackendDriverState {
        self.driver.state.borrow().transport.driver_state()
    }

    pub fn audio_context(&self) -> Option<AudioContext> {
        self.driver.state.borrow().context.clone()
    }

    pub fn set_repaint_context(&self, context: egui::Context) {
        self.driver.state.borrow_mut().repaint_context = Some(context);
    }

    pub fn update_presentation(&self) {
        let (state, generation, owned_media_tracks, input_mode) = {
            let mut inner = self.driver.state.borrow_mut();
            let mut state = inner.transport.driver_state();
            if matches!(
                state,
                BackendDriverState::Starting | BackendDriverState::Suspended
            ) && inner
                .startup_started
                .is_some_and(|started| started.elapsed() >= Duration::from_secs(15))
            {
                inner.transport.fail("browser audio startup timed out");
                state = BackendDriverState::Failed;
            }
            if state == BackendDriverState::Failed && inner.context.is_some() {
                shutdown_graph(&mut inner);
            }
            let owned_media_tracks = inner
                .stream
                .as_ref()
                .map(|stream| stream.get_tracks().length())
                .unwrap_or(0);
            (
                state,
                inner.generation,
                owned_media_tracks,
                inner.input_mode,
            )
        };
        if let Some(element) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id("runtime_status"))
        {
            let _ = element.set_attribute("data-audio-generation", &generation.to_string());
            let _ =
                element.set_attribute("data-owned-media-tracks", &owned_media_tracks.to_string());
        }
        let busy = matches!(
            state,
            BackendDriverState::RequestingPermission | BackendDriverState::Starting
        );
        let output_enabled = matches!(
            state,
            BackendDriverState::Running | BackendDriverState::Suspended
        );
        let microphone_enabled = output_enabled && input_mode == Some(AudioInputMode::Microphone);
        for (button, mode, enabled, enable_text, retry_text) in [
            (
                microphone_enable_button(),
                AudioInputMode::Microphone,
                microphone_enabled,
                "Enable microphone audio",
                "Retry microphone audio",
            ),
            (
                output_enable_button(),
                AudioInputMode::OutputOnly,
                output_enabled,
                "Enable output-only audio",
                "Retry output-only audio",
            ),
        ] {
            if let Ok(button) = button {
                button.set_hidden(enabled);
                button.set_disabled(busy || state == BackendDriverState::Unsupported);
                button.set_text_content(Some(
                    if input_mode == Some(mode)
                        && matches!(
                            state,
                            BackendDriverState::Denied
                                | BackendDriverState::Failed
                                | BackendDriverState::Stopped
                        )
                    {
                        retry_text
                    } else {
                        enable_text
                    },
                ));
            }
        }
        let output_status = match state {
            BackendDriverState::RequestingPermission | BackendDriverState::Starting => "Starting…",
            BackendDriverState::Running => "Enabled",
            BackendDriverState::Suspended => "Enabled (suspended)",
            BackendDriverState::Unsupported => "Unavailable in this browser",
            BackendDriverState::Failed => "Failed",
            _ => "Not enabled",
        };
        let microphone_status = match (state, input_mode) {
            (BackendDriverState::RequestingPermission, Some(AudioInputMode::Microphone)) => {
                "Requesting permission…"
            }
            (BackendDriverState::Starting, Some(AudioInputMode::Microphone))
            | (BackendDriverState::Running, Some(AudioInputMode::Microphone))
            | (BackendDriverState::Suspended, Some(AudioInputMode::Microphone)) => "Granted",
            (BackendDriverState::Denied, Some(AudioInputMode::Microphone)) => "Denied",
            (BackendDriverState::Failed, Some(AudioInputMode::Microphone)) => "Failed",
            (BackendDriverState::Unsupported, _) => "Unavailable in this browser",
            _ => "Not granted",
        };
        set_permission_status("audio_output_permission_status", output_status);
        set_permission_status("microphone_permission_status", microphone_status);
    }

    pub fn shutdown(&self) {
        shutdown_inner(&mut self.driver.state.borrow_mut());
    }
}

impl Drop for BrowserAudioController {
    fn drop(&mut self) {
        let _ = (
            &self.presentation.microphone_enable_handler,
            &self.presentation.output_enable_handler,
            &self.presentation.suspend_handler,
            &self.presentation.resume_handler,
            &self.presentation.fail_handler,
            &self.presentation.track_end_handler,
            &self.presentation.shutdown_handler,
        );
        if let Ok(button) = microphone_enable_button() {
            button.set_onclick(None);
        }
        if let Ok(button) = output_enable_button() {
            button.set_onclick(None);
        }
        if let Some(window) = web_sys::window() {
            let _ = Reflect::delete_property(window.as_ref(), &"shoopAudioDiagnostics".into());
        }
        self.shutdown();
    }
}

fn audio_button(id: &str) -> Result<HtmlButtonElement> {
    web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(id))
        .ok_or_else(|| anyhow!("missing #{id} button"))?
        .dyn_into::<HtmlButtonElement>()
        .map_err(|_| anyhow!("#{id} is not a button"))
}

fn microphone_enable_button() -> Result<HtmlButtonElement> {
    audio_button("enable_audio")
}

fn output_enable_button() -> Result<HtmlButtonElement> {
    audio_button("enable_output_audio")
}

fn set_permission_status(id: &str, status: &str) {
    if let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(id))
    {
        element.set_text_content(Some(status));
    }
}

fn begin_enable(inner: Rc<RefCell<PhysicalAudioDriverState>>, input_mode: AudioInputMode) {
    let (state, current_mode) = {
        let inner = inner.borrow();
        let state = inner.transport.driver_state();
        (state, inner.input_mode)
    };
    let microphone_upgrade = matches!(
        (state, current_mode, input_mode),
        (
            BackendDriverState::Running | BackendDriverState::Suspended,
            Some(AudioInputMode::OutputOnly),
            AudioInputMode::Microphone
        )
    );
    if matches!(
        state,
        BackendDriverState::RequestingPermission
            | BackendDriverState::Starting
            | BackendDriverState::Running
            | BackendDriverState::Suspended
    ) && !microphone_upgrade
    {
        return;
    }
    let Some(window) = web_sys::window() else {
        inner
            .borrow()
            .transport
            .fail("browser window is unavailable");
        return;
    };
    inner.borrow_mut().input_mode = Some(input_mode);
    let context = match AudioContext::new() {
        Ok(context) => context,
        Err(error) => {
            inner
                .borrow()
                .transport
                .fail(format!("AudioContext is unavailable: {error:?}"));
            return;
        }
    };
    let resume = match context.resume() {
        Ok(resume) => resume,
        Err(error) => {
            let _ = context.close();
            inner
                .borrow()
                .transport
                .fail(format!("could not begin AudioContext resume: {error:?}"));
            return;
        }
    };
    let media = if input_mode == AudioInputMode::Microphone {
        let constraints = microphone_constraints();
        match window
            .navigator()
            .media_devices()
            .and_then(|devices| devices.get_user_media_with_constraints(&constraints))
        {
            Ok(media) => Some(media),
            Err(error) => {
                let _ = context.close();
                inner
                    .borrow()
                    .transport
                    .fail(format!("getUserMedia is unavailable: {error:?}"));
                return;
            }
        }
    } else {
        None
    };

    let generation = {
        let mut inner = inner.borrow_mut();
        shutdown_inner(&mut inner);
        inner.generation = inner.generation.saturating_add(1);
        inner.startup_started = Some(Instant::now());
        inner.context = Some(context.clone());
        inner.input_mode = Some(input_mode);
        inner
            .transport
            .set_driver_state(if input_mode == AudioInputMode::Microphone {
                BackendDriverState::RequestingPermission
            } else {
                BackendDriverState::Starting
            });
        inner.generation
    };

    wasm_bindgen_futures::spawn_local(async move {
        let result = start_audio_graph(inner.clone(), generation, context, resume, media).await;
        if let Err(error) = result {
            let state = if js_error_name(&error) == "NotAllowedError" {
                BackendDriverState::Denied
            } else {
                BackendDriverState::Failed
            };
            let mut inner = inner.borrow_mut();
            if inner.generation == generation {
                if state == BackendDriverState::Denied {
                    inner.transport.set_driver_state(state);
                } else {
                    inner.transport.fail(js_error_message(&error));
                }
                shutdown_graph(&mut inner);
            }
        }
    });
}

async fn start_audio_graph(
    inner: Rc<RefCell<PhysicalAudioDriverState>>,
    generation: u64,
    context: AudioContext,
    resume: js_sys::Promise,
    media: Option<js_sys::Promise>,
) -> std::result::Result<(), JsValue> {
    let stream = match media {
        Some(media) => {
            let stream = JsFuture::from(media).await?.dyn_into::<MediaStream>()?;
            publish_track_settings(&stream);
            Some(stream)
        }
        None => None,
    };
    if inner.borrow().generation != generation {
        if let Some(stream) = &stream {
            stop_stream(stream);
        }
        return Ok(());
    }
    {
        let mut inner = inner.borrow_mut();
        inner.startup_started = Some(Instant::now());
        inner
            .transport
            .set_driver_state(BackendDriverState::Starting);
    }

    let module = load_worklet_module(&context).await?;

    let processor_options = Object::new();
    Reflect::set(&processor_options, &"wasmModule".into(), module.as_ref())?;
    Reflect::set(
        &processor_options,
        &"maxQuantum".into(),
        &JsValue::from_f64(MAX_QUANTUM as f64),
    )?;
    Reflect::set(
        &processor_options,
        &"protocolVersion".into(),
        &JsValue::from_f64(PROTOCOL_VERSION as f64),
    )?;
    Reflect::set(
        &processor_options,
        &"commandMaxBytes".into(),
        &JsValue::from_f64(COMMAND_MAX_BYTES as f64),
    )?;
    let options = AudioWorkletNodeOptions::new();
    options.set_number_of_inputs(if stream.is_some() { 1 } else { 0 });
    options.set_number_of_outputs(1);
    let output_channels = Array::new();
    output_channels.push(&JsValue::from_f64(MAX_DEVICE_AUDIO_CHANNELS as f64));
    options.set_output_channel_count(&output_channels.into());
    options.set_processor_options(Some(&processor_options));
    let node = AudioWorkletNode::new_with_options(&context, WORKLET_NAME, &options)?;
    let source = if let Some(stream) = &stream {
        let source = context.create_media_stream_source(stream)?;
        source.connect_with_audio_node(&node)?;
        Some(source)
    } else {
        None
    };
    node.connect_with_audio_node(&context.destination())?;

    let port = node.port()?;
    let weak = Rc::downgrade(&inner);
    let message_handler = Closure::wrap(Box::new(move |event: MessageEvent| {
        if let Some(inner) = weak.upgrade() {
            let inner = inner.borrow();
            if let Some(json) = event.data().as_string() {
                let _ = inner.transport.receive(generation, &json);
            } else {
                inner.transport.fail("worklet emitted a non-string event");
            }
            if let Some(context) = &inner.repaint_context {
                context.request_repaint();
            }
        }
    }) as Box<dyn FnMut(_)>);
    port.set_onmessage(Some(message_handler.as_ref().unchecked_ref()));
    port.start();

    let weak = Rc::downgrade(&inner);
    let processor_error_handler = Closure::wrap(Box::new(move |_event: WebEvent| {
        if let Some(inner) = weak.upgrade() {
            let inner = inner.borrow();
            if inner.generation == generation {
                inner.transport.fail("AudioWorklet processor terminated");
            }
        }
    }) as Box<dyn FnMut(_)>);
    node.set_onprocessorerror(Some(processor_error_handler.as_ref().unchecked_ref()));

    let weak = Rc::downgrade(&inner);
    let context_for_state = context.clone();
    let context_state_handler = Closure::wrap(Box::new(move |_event: WebEvent| {
        if let Some(inner) = weak.upgrade() {
            let mut inner = inner.borrow_mut();
            if inner.generation != generation {
                return;
            }
            let state = match context_for_state.state() {
                AudioContextState::Running => BackendDriverState::Running,
                AudioContextState::Suspended => BackendDriverState::Suspended,
                AudioContextState::Closed => BackendDriverState::Stopped,
                _ => BackendDriverState::Failed,
            };
            if state == BackendDriverState::Running {
                inner.startup_started = None;
            }
            inner.transport.set_driver_state(state);
        }
    }) as Box<dyn FnMut(_)>);
    context.set_onstatechange(Some(context_state_handler.as_ref().unchecked_ref()));

    let mut track_ended_handlers = Vec::new();
    for value in stream
        .as_ref()
        .map(MediaStream::get_tracks)
        .unwrap_or_default()
        .iter()
    {
        let track = value.dyn_into::<MediaStreamTrack>()?;
        let weak = Rc::downgrade(&inner);
        let handler = Closure::wrap(Box::new(move |_event: WebEvent| {
            if let Some(inner) = weak.upgrade() {
                let inner = inner.borrow();
                if inner.generation == generation {
                    inner.transport.fail("microphone media track ended");
                }
            }
        }) as Box<dyn FnMut(_)>);
        track.set_onended(Some(handler.as_ref().unchecked_ref()));
        track_ended_handlers.push(handler);
    }

    if inner.borrow().generation != generation {
        if let Some(stream) = &stream {
            stop_stream(stream);
        }
        return Ok(());
    }
    {
        let input_channels = stream
            .as_ref()
            .and_then(capture_channel_count)
            .unwrap_or(u32::from(stream.is_some()))
            .min(MAX_DEVICE_AUDIO_CHANNELS as u32);
        let output_channels = context
            .destination()
            .channel_count()
            .min(MAX_DEVICE_AUDIO_CHANNELS as u32);
        let mut inner = inner.borrow_mut();
        inner.stream = stream;
        inner.source = source;
        inner.node = Some(node);
        inner.message_handler = Some(message_handler);
        inner.processor_error_handler = Some(processor_error_handler);
        inner.context_state_handler = Some(context_state_handler);
        inner.track_ended_handlers = track_ended_handlers;
        inner
            .transport
            .attach(
                Box::new(BrowserMessageEndpoint(port)),
                generation,
                input_channels,
                output_channels,
            )
            .map_err(|error| {
                JsValue::from_str(&format!("could not initialize worklet protocol: {error}"))
            })?;
        let state = if context.state() == AudioContextState::Running {
            inner.startup_started = None;
            BackendDriverState::Running
        } else {
            BackendDriverState::Suspended
        };
        inner.transport.set_driver_state(state);
    }
    let weak = Rc::downgrade(&inner);
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(error) = JsFuture::from(resume).await {
            if let Some(inner) = weak.upgrade() {
                let inner = inner.borrow();
                if inner.generation == generation {
                    inner.transport.fail(js_error_message(&error));
                }
            }
        }
    });
    Ok(())
}

async fn load_worklet_module(
    context: &AudioContext,
) -> std::result::Result<WebAssembly::Module, JsValue> {
    let window =
        web_sys::window().ok_or_else(|| JsValue::from_str("browser window disappeared"))?;
    let embedded = Reflect::get(window.as_ref(), &EMBEDDED_WORKLET_ASSETS.into())?;
    if !embedded.is_null() && !embedded.is_undefined() {
        let module_url = Reflect::get(&embedded, &"moduleUrl".into())?
            .as_string()
            .ok_or_else(|| JsValue::from_str("embedded AudioWorklet module URL is invalid"))?;
        let wasm_bytes = Reflect::get(&embedded, &"wasmBytes".into())?;
        JsFuture::from(context.audio_worklet()?.add_module(&module_url)?).await?;
        return JsFuture::from(WebAssembly::compile(&wasm_bytes))
            .await?
            .dyn_into::<WebAssembly::Module>();
    }

    JsFuture::from(context.audio_worklet()?.add_module(WORKLET_SCRIPT_URL)?).await?;
    let response = JsFuture::from(window.fetch_with_str(WORKLET_WASM_URL))
        .await?
        .dyn_into::<Response>()?;
    if !response.ok() {
        return Err(JsValue::from_str(
            "could not fetch the AudioWorklet Wasm module",
        ));
    }
    let bytes = JsFuture::from(response.array_buffer()?).await?;
    JsFuture::from(WebAssembly::compile(&bytes))
        .await?
        .dyn_into::<WebAssembly::Module>()
}

fn microphone_constraints() -> MediaStreamConstraints {
    let raw = Object::new();
    let _ = Reflect::set(&raw, &"echoCancellation".into(), &JsValue::FALSE);
    let _ = Reflect::set(&raw, &"noiseSuppression".into(), &JsValue::FALSE);
    let _ = Reflect::set(&raw, &"autoGainControl".into(), &JsValue::FALSE);
    let constraints = MediaStreamConstraints::new();
    constraints.set_audio(&raw.into());
    constraints
}

fn js_error_name(error: &JsValue) -> String {
    Reflect::get(error, &"name".into())
        .ok()
        .and_then(|name| name.as_string())
        .unwrap_or_default()
}

fn js_error_message(error: &JsValue) -> String {
    Reflect::get(error, &"message".into())
        .ok()
        .and_then(|message| message.as_string())
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| format!("browser audio startup failed: {error:?}"))
}

fn capture_channel_count(stream: &MediaStream) -> Option<u32> {
    stream
        .get_audio_tracks()
        .iter()
        .next()
        .and_then(|value| value.dyn_into::<MediaStreamTrack>().ok())
        .and_then(|track| track.get_settings().get_channel_count())
        .and_then(|channels| u32::try_from(channels).ok())
}

fn publish_track_settings(stream: &MediaStream) {
    let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("runtime_status"))
    else {
        return;
    };
    let Some(value) = stream.get_audio_tracks().iter().next() else {
        return;
    };
    let Ok(track) = value.dyn_into::<MediaStreamTrack>() else {
        return;
    };
    let settings = track.get_settings();
    if let Some(channels) = settings.get_channel_count() {
        let _ = element.set_attribute("data-capture-channels", &channels.to_string());
    }
    if let Some(enabled) = settings.get_echo_cancellation() {
        let _ = element.set_attribute("data-echo-cancellation", &enabled.to_string());
    }
    if let Some(enabled) = settings.get_noise_suppression() {
        let _ = element.set_attribute("data-noise-suppression", &enabled.to_string());
    }
    if let Some(enabled) = settings.get_auto_gain_control() {
        let _ = element.set_attribute("data-auto-gain-control", &enabled.to_string());
    }
}

fn stop_stream(stream: &MediaStream) {
    for value in stream.get_tracks().iter() {
        if let Ok(track) = value.dyn_into::<MediaStreamTrack>() {
            track.stop();
        }
    }
}

fn shutdown_graph(inner: &mut PhysicalAudioDriverState) {
    inner.startup_started = None;
    if let Some(stream) = inner.stream.take() {
        for value in stream.get_tracks().iter() {
            if let Ok(track) = value.dyn_into::<MediaStreamTrack>() {
                track.set_onended(None);
            }
        }
        stop_stream(&stream);
    }
    if let Some(source) = inner.source.take() {
        let _ = source.disconnect();
    }
    if let Some(node) = inner.node.take() {
        node.set_onprocessorerror(None);
        if let Ok(port) = node.port() {
            port.set_onmessage(None);
            port.close();
        }
        let _ = node.disconnect();
    }
    if let Some(context) = inner.context.take() {
        context.set_onstatechange(None);
        let _ = context.close();
    }
    inner.message_handler = None;
    inner.processor_error_handler = None;
    inner.context_state_handler = None;
    inner.track_ended_handlers.clear();
}

fn shutdown_inner(inner: &mut PhysicalAudioDriverState) {
    shutdown_graph(inner);
    inner.transport.detach(false);
    if !matches!(
        inner.transport.driver_state(),
        BackendDriverState::Denied | BackendDriverState::Unsupported | BackendDriverState::Failed
    ) {
        inner
            .transport
            .set_driver_state(BackendDriverState::Stopped);
    }
}
