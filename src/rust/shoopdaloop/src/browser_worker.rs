use anyhow::{anyhow, Result};
use js_sys::{Array, Object, Reflect, WebAssembly};
use shoop_audio_protocol::{COMMAND_MAX_BYTES, PROTOCOL_VERSION};
use shoop_backend::BackendDriverState;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Event as WebEvent, MessageChannel, MessageEvent, MessagePort, Response, Worker};

const WORKER_SCRIPT_URL: &str = "./audio_worker.js";
const WORKLET_WASM_URL: &str = "./generated/shoop_audio_worklet.wasm";
const EMBEDDED_WORKLET_ASSETS: &str = "shoopEmbeddedAudioWorklet";
const MAX_QUANTUM: u32 = 2048;
const GENERATION: u64 = 1;

struct BrowserWorkerEndpoint(MessagePort);

impl shoop_worklet_client::MessageEndpoint for BrowserWorkerEndpoint {
    fn post_message(&self, message: &str) -> Result<()> {
        self.0
            .post_message(&JsValue::from_str(message))
            .map_err(|error| anyhow!("could not post worker engine command: {error:?}"))
    }

    fn close(&self) {
        self.0.close();
    }
}

/// Browser engine driver that owns a Dedicated Worker and no audio hardware.
pub struct BrowserWorkerDriver {
    transport: shoop_worklet_client::RemoteBackendControl,
    worker: Worker,
    application_port: MessagePort,
    message_handler: Closure<dyn FnMut(MessageEvent)>,
    error_handler: Closure<dyn FnMut(WebEvent)>,
}

impl BrowserWorkerDriver {
    pub fn new(transport: shoop_worklet_client::RemoteBackendControl) -> Result<Self> {
        let window = web_sys::window().ok_or_else(|| anyhow!("browser window is unavailable"))?;
        let embedded = Reflect::get(window.as_ref(), &EMBEDDED_WORKLET_ASSETS.into())
            .map_err(|error| anyhow!("could not inspect embedded worker assets: {error:?}"))?;
        let worker_url = if embedded.is_null() || embedded.is_undefined() {
            WORKER_SCRIPT_URL.to_owned()
        } else {
            Reflect::get(&embedded, &"workerUrl".into())
                .ok()
                .and_then(|value| value.as_string())
                .unwrap_or_else(|| WORKER_SCRIPT_URL.to_owned())
        };
        let worker = Worker::new(&worker_url)
            .map_err(|error| anyhow!("could not create browser engine Worker: {error:?}"))?;

        let channel = MessageChannel::new()
            .map_err(|error| anyhow!("could not create Worker application channel: {error:?}"))?;
        let application_port = channel.port1();
        let worker_port = channel.port2();
        let receive_control = transport.clone();
        let message_handler = Closure::wrap(Box::new(move |event: MessageEvent| {
            if let Some(json) = event.data().as_string() {
                let _ = receive_control.receive(GENERATION, &json);
            } else {
                receive_control.fail("worker engine emitted a non-string event");
            }
        }) as Box<dyn FnMut(_)>);
        application_port.set_onmessage(Some(message_handler.as_ref().unchecked_ref()));
        application_port.start();

        let failure_control = transport.clone();
        let error_handler = Closure::wrap(Box::new(move |_event: WebEvent| {
            failure_control.fail("browser engine Worker terminated unexpectedly");
        }) as Box<dyn FnMut(_)>);
        worker.set_onerror(Some(error_handler.as_ref().unchecked_ref()));

        transport.set_driver_state(BackendDriverState::Starting);
        let initialize_worker = worker.clone();
        let initialize_port = application_port.clone();
        let initialize_control = transport.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match load_engine_module().await {
                Ok(module) => {
                    let options = Object::new();
                    let result = Reflect::set(&options, &"kind".into(), &"initialize".into())
                        .and_then(|_| Reflect::set(&options, &"wasmModule".into(), module.as_ref()))
                        .and_then(|_| {
                            Reflect::set(&options, &"applicationPort".into(), worker_port.as_ref())
                        })
                        .and_then(|_| {
                            Reflect::set(
                                &options,
                                &"sampleRate".into(),
                                &JsValue::from_f64(48_000.0),
                            )
                        })
                        .and_then(|_| {
                            Reflect::set(&options, &"quantum".into(), &JsValue::from_f64(128.0))
                        })
                        .and_then(|_| {
                            Reflect::set(&options, &"processingMode".into(), &"realtime".into())
                        })
                        .and_then(|_| {
                            Reflect::set(
                                &options,
                                &"maxQuantum".into(),
                                &JsValue::from_f64(MAX_QUANTUM as f64),
                            )
                        })
                        .and_then(|_| {
                            Reflect::set(
                                &options,
                                &"protocolVersion".into(),
                                &JsValue::from_f64(PROTOCOL_VERSION as f64),
                            )
                        })
                        .and_then(|_| {
                            Reflect::set(
                                &options,
                                &"commandMaxBytes".into(),
                                &JsValue::from_f64(COMMAND_MAX_BYTES as f64),
                            )
                        })
                        .and_then(|_| {
                            let transfer = Array::new();
                            transfer.push(worker_port.as_ref());
                            initialize_worker.post_message_with_transfer(&options, &transfer)
                        });
                    if let Err(error) = result {
                        initialize_control.fail(format!(
                            "could not initialize browser engine Worker: {error:?}"
                        ));
                        return;
                    }
                    if let Err(error) = initialize_control.attach(
                        Box::new(BrowserWorkerEndpoint(initialize_port)),
                        GENERATION,
                        0,
                        0,
                    ) {
                        initialize_control
                            .fail(format!("could not attach browser engine Worker: {error}"));
                        return;
                    }
                    initialize_control.set_driver_state(BackendDriverState::Dummy);
                }
                Err(error) => initialize_control
                    .fail(format!("could not load browser engine Worker: {error:?}")),
            }
        });

        Ok(Self {
            transport,
            worker,
            application_port,
            message_handler,
            error_handler,
        })
    }

    pub fn state(&self) -> BackendDriverState {
        self.transport.driver_state()
    }

    pub fn update_presentation(&self) {
        let _ = (&self.message_handler, &self.error_handler);
        if let Some(document) = web_sys::window().and_then(|window| window.document()) {
            for id in ["enable_audio", "enable_output_audio"] {
                if let Some(element) = document.get_element_by_id(id) {
                    let _ = element.set_attribute("hidden", "");
                }
            }
            for id in [
                "audio_output_permission_status",
                "microphone_permission_status",
            ] {
                if let Some(element) = document.get_element_by_id(id) {
                    element.set_text_content(Some("Not used by Worker engine"));
                }
            }
        }
    }

    pub fn shutdown(&self) {
        self.transport.detach(false);
        self.transport.set_driver_state(BackendDriverState::Stopped);
        self.worker.terminate();
    }
}

impl Drop for BrowserWorkerDriver {
    fn drop(&mut self) {
        self.application_port.set_onmessage(None);
        self.worker.set_onerror(None);
        self.shutdown();
    }
}

async fn load_engine_module() -> std::result::Result<WebAssembly::Module, JsValue> {
    let window =
        web_sys::window().ok_or_else(|| JsValue::from_str("browser window disappeared"))?;
    let embedded = Reflect::get(window.as_ref(), &EMBEDDED_WORKLET_ASSETS.into())?;
    let bytes = if embedded.is_null() || embedded.is_undefined() {
        let response = JsFuture::from(window.fetch_with_str(WORKLET_WASM_URL))
            .await?
            .dyn_into::<Response>()?;
        if !response.ok() {
            return Err(JsValue::from_str(
                "could not fetch the Worker engine Wasm module",
            ));
        }
        JsFuture::from(response.array_buffer()?).await?
    } else {
        Reflect::get(&embedded, &"wasmBytes".into())?
    };
    JsFuture::from(WebAssembly::compile(&bytes))
        .await?
        .dyn_into::<WebAssembly::Module>()
}
