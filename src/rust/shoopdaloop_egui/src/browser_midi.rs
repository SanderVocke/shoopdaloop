use std::collections::{BTreeMap, VecDeque};

#[cfg(target_arch = "wasm32")]
use shoop_scripting::{MidiConnectionId, MidiControlService};
use shoop_scripting::{
    MidiEndpoint, MidiEndpointDirection, MidiEndpointSnapshot, MAX_MIDI_MESSAGE_BYTES,
};

pub const TRACK_MIDI_MESSAGE_BYTES: usize = 4;
pub const MIDI_INPUT_QUEUE_CAPACITY: usize = 1024;
pub const MIDI_TRACK_QUEUE_CAPACITY: usize = 1024;
pub const MIDI_SUBSCRIPTION_CAPACITY: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrowserMidiState {
    Unsupported,
    AwaitingGesture,
    RequestingPermission,
    Running,
    Denied,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserMidiEndpoint {
    pub id: String,
    pub raw_id: String,
    pub name: String,
    pub direction: MidiEndpointDirection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackMidiInput {
    pub endpoint_id: String,
    pub data: Vec<u8>,
}

struct InputSubscription {
    endpoint_id: String,
    messages: VecDeque<Vec<u8>>,
    dropped: u32,
}

pub struct BrowserMidiCore {
    state: BrowserMidiState,
    error: Option<String>,
    sysex_enabled: bool,
    revision: u64,
    endpoints: BTreeMap<String, BrowserMidiEndpoint>,
    subscriptions: BTreeMap<u64, InputSubscription>,
    next_subscription: u64,
    track_messages: VecDeque<TrackMidiInput>,
    dropped_track_messages: u32,
    refused_track_messages: u32,
    refused_control_messages: u32,
}

impl BrowserMidiCore {
    pub fn new(supported: bool) -> Self {
        Self {
            state: if supported {
                BrowserMidiState::AwaitingGesture
            } else {
                BrowserMidiState::Unsupported
            },
            error: None,
            sysex_enabled: false,
            revision: 0,
            endpoints: BTreeMap::new(),
            subscriptions: BTreeMap::new(),
            next_subscription: 1,
            track_messages: VecDeque::with_capacity(MIDI_TRACK_QUEUE_CAPACITY),
            dropped_track_messages: 0,
            refused_track_messages: 0,
            refused_control_messages: 0,
        }
    }

    pub fn state(&self) -> BrowserMidiState {
        self.state
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn sysex_enabled(&self) -> bool {
        self.sysex_enabled
    }

    pub fn report_error(&mut self, error: String) {
        self.error = Some(error);
    }

    pub fn set_state(
        &mut self,
        state: BrowserMidiState,
        error: Option<String>,
        sysex_enabled: bool,
    ) {
        self.state = state;
        self.error = error;
        self.sysex_enabled = sysex_enabled;
        if state != BrowserMidiState::Running {
            self.replace_endpoints(Vec::new());
        }
    }

    pub fn replace_endpoints(&mut self, endpoints: Vec<BrowserMidiEndpoint>) {
        let replacement = endpoints
            .into_iter()
            .map(|endpoint| (endpoint.id.clone(), endpoint))
            .collect::<BTreeMap<_, _>>();
        if replacement != self.endpoints {
            self.endpoints = replacement;
            self.revision = self.revision.wrapping_add(1);
        }
    }

    pub fn endpoint_snapshot(&self) -> MidiEndpointSnapshot {
        MidiEndpointSnapshot {
            revision: self.revision,
            endpoints: self
                .endpoints
                .values()
                .map(|endpoint| MidiEndpoint {
                    id: endpoint.id.clone(),
                    name: endpoint.name.clone(),
                    direction: endpoint.direction,
                })
                .collect(),
        }
    }

    pub fn endpoints(&self) -> Vec<BrowserMidiEndpoint> {
        self.endpoints.values().cloned().collect()
    }

    pub fn endpoint(&self, id: &str) -> Option<&BrowserMidiEndpoint> {
        self.endpoints.get(id)
    }

    pub fn subscribe_input(&mut self, endpoint_id: &str) -> anyhow::Result<u64> {
        if self.subscriptions.len() >= MIDI_SUBSCRIPTION_CAPACITY {
            anyhow::bail!("Web MIDI input subscription capacity exhausted");
        }
        let endpoint = self
            .endpoints
            .get(endpoint_id)
            .ok_or_else(|| anyhow::anyhow!("Web MIDI input endpoint disappeared: {endpoint_id}"))?;
        if endpoint.direction != MidiEndpointDirection::Output {
            anyhow::bail!("Web MIDI endpoint is not an input source: {endpoint_id}");
        }
        let id = self.next_subscription;
        self.next_subscription = self.next_subscription.saturating_add(1);
        self.subscriptions.insert(
            id,
            InputSubscription {
                endpoint_id: endpoint_id.to_owned(),
                messages: VecDeque::with_capacity(MIDI_INPUT_QUEUE_CAPACITY),
                dropped: 0,
            },
        );
        Ok(id)
    }

    pub fn unsubscribe_input(&mut self, subscription: u64) {
        self.subscriptions.remove(&subscription);
    }

    pub fn receive(&mut self, endpoint_id: &str, data: &[u8]) {
        if data.is_empty() || data.len() > MAX_MIDI_MESSAGE_BYTES {
            self.refused_control_messages = self.refused_control_messages.saturating_add(1);
            if data.len() > TRACK_MIDI_MESSAGE_BYTES || data.is_empty() {
                self.refused_track_messages = self.refused_track_messages.saturating_add(1);
            }
            return;
        }
        if !self
            .endpoints
            .get(endpoint_id)
            .is_some_and(|endpoint| endpoint.direction == MidiEndpointDirection::Output)
        {
            return;
        }
        for subscription in self
            .subscriptions
            .values_mut()
            .filter(|subscription| subscription.endpoint_id == endpoint_id)
        {
            if subscription.messages.len() >= MIDI_INPUT_QUEUE_CAPACITY {
                subscription.dropped = subscription.dropped.saturating_add(1);
            } else {
                subscription.messages.push_back(data.to_vec());
            }
        }
        if data.len() > TRACK_MIDI_MESSAGE_BYTES {
            self.refused_track_messages = self.refused_track_messages.saturating_add(1);
        } else if self.track_messages.len() >= MIDI_TRACK_QUEUE_CAPACITY {
            self.dropped_track_messages = self.dropped_track_messages.saturating_add(1);
        } else {
            self.track_messages.push_back(TrackMidiInput {
                endpoint_id: endpoint_id.to_owned(),
                data: data.to_vec(),
            });
        }
    }

    pub fn drain_subscription(
        &mut self,
        subscription: u64,
        max_messages: usize,
    ) -> anyhow::Result<Vec<Vec<u8>>> {
        let subscription = self
            .subscriptions
            .get_mut(&subscription)
            .ok_or_else(|| anyhow::anyhow!("unknown Web MIDI input subscription"))?;
        let count = max_messages.min(subscription.messages.len());
        Ok(subscription.messages.drain(..count).collect())
    }

    pub fn take_subscription_dropped(&mut self, subscription: u64) -> u32 {
        self.subscriptions
            .get_mut(&subscription)
            .map(|subscription| std::mem::take(&mut subscription.dropped))
            .unwrap_or(0)
    }

    pub fn drain_track_messages(&mut self, max_messages: usize) -> Vec<TrackMidiInput> {
        let count = max_messages.min(self.track_messages.len());
        self.track_messages.drain(..count).collect()
    }

    pub fn dropped_track_messages(&self) -> u32 {
        self.dropped_track_messages
    }

    pub fn refused_track_messages(&self) -> u32 {
        self.refused_track_messages
    }

    pub fn refused_control_messages(&self) -> u32 {
        self.refused_control_messages
    }
}

#[cfg(target_arch = "wasm32")]
mod platform {
    use std::cell::RefCell;
    use std::rc::{Rc, Weak};

    use anyhow::anyhow;
    use js_sys::{Reflect, Uint8Array};
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::{spawn_local, JsFuture};
    use web_sys::{
        Event, HtmlButtonElement, MidiAccess, MidiInput, MidiMessageEvent, MidiOptions, MidiOutput,
        MidiPort, MidiPortDeviceState,
    };

    use super::*;

    const ENABLE_BUTTON_ID: &str = "enable_midi";

    struct InputHandle {
        port: MidiInput,
        _handler: Closure<dyn FnMut(MidiMessageEvent)>,
    }

    struct HubInner {
        core: BrowserMidiCore,
        access: Option<MidiAccess>,
        inputs: BTreeMap<String, InputHandle>,
        outputs: BTreeMap<String, MidiOutput>,
        state_handler: Option<Closure<dyn FnMut(Event)>>,
    }

    impl HubInner {
        fn refresh(inner: &Rc<RefCell<Self>>) -> anyhow::Result<()> {
            let access = inner
                .borrow()
                .access
                .clone()
                .ok_or_else(|| anyhow!("Web MIDI access is unavailable"))?;
            let mut endpoints = Vec::new();
            {
                let mut state = inner.borrow_mut();
                for handle in state.inputs.values() {
                    handle.port.set_onmidimessage(None);
                    let port: &MidiPort = handle.port.unchecked_ref();
                    let _ = port.close();
                }
                for output in state.outputs.values() {
                    let port: &MidiPort = output.unchecked_ref();
                    let _ = port.close();
                }
                state.inputs.clear();
                state.outputs.clear();
            }
            let mut inputs = BTreeMap::new();
            for value in iterator_values(access.inputs().values().into())? {
                let port = value
                    .dyn_into::<MidiInput>()
                    .map_err(|_| anyhow!("Web MIDI input map contained an invalid port"))?;
                let midi_port: &MidiPort = port.unchecked_ref();
                if midi_port.state() != MidiPortDeviceState::Connected {
                    continue;
                }
                let raw_id = midi_port.id();
                let id = endpoint_id(MidiEndpointDirection::Output, &raw_id);
                endpoints.push(BrowserMidiEndpoint {
                    id: id.clone(),
                    raw_id,
                    name: endpoint_name(midi_port),
                    direction: MidiEndpointDirection::Output,
                });
                let weak = Rc::downgrade(inner);
                let callback_id = id.clone();
                let handler = Closure::wrap(Box::new(move |event: MidiMessageEvent| {
                    if let Some(inner) = weak.upgrade() {
                        if let Ok(data) = event.data() {
                            inner.borrow_mut().core.receive(&callback_id, &data);
                        }
                    }
                }) as Box<dyn FnMut(_)>);
                port.set_onmidimessage(Some(handler.as_ref().unchecked_ref()));
                observe_open(inner, midi_port.open(), "input", &id);
                inputs.insert(
                    id,
                    InputHandle {
                        port,
                        _handler: handler,
                    },
                );
            }
            let mut outputs = BTreeMap::new();
            for value in iterator_values(access.outputs().values().into())? {
                let port = value
                    .dyn_into::<MidiOutput>()
                    .map_err(|_| anyhow!("Web MIDI output map contained an invalid port"))?;
                let midi_port: &MidiPort = port.unchecked_ref();
                if midi_port.state() != MidiPortDeviceState::Connected {
                    continue;
                }
                let raw_id = midi_port.id();
                let id = endpoint_id(MidiEndpointDirection::Input, &raw_id);
                endpoints.push(BrowserMidiEndpoint {
                    id: id.clone(),
                    raw_id,
                    name: endpoint_name(midi_port),
                    direction: MidiEndpointDirection::Input,
                });
                observe_open(inner, midi_port.open(), "output", &id);
                outputs.insert(id, port);
            }
            let mut state = inner.borrow_mut();
            state.inputs = inputs;
            state.outputs = outputs;
            state.core.replace_endpoints(endpoints);
            Ok(())
        }
    }

    fn iterator_values(iterator: js_sys::Iterator) -> anyhow::Result<Vec<JsValue>> {
        let Some(iterator) = js_sys::try_iter(&iterator)
            .map_err(|error| anyhow!("could not iterate MIDI ports: {error:?}"))?
        else {
            return Err(anyhow!("Web MIDI port map is not iterable"));
        };
        let mut values = Vec::new();
        for value in iterator {
            values.push(value.map_err(|error| anyhow!("could not read MIDI port: {error:?}"))?);
        }
        Ok(values)
    }

    fn observe_open(
        inner: &Rc<RefCell<HubInner>>,
        promise: js_sys::Promise,
        kind: &'static str,
        endpoint_id: &str,
    ) {
        let weak = Rc::downgrade(inner);
        let endpoint_id = endpoint_id.to_owned();
        spawn_local(async move {
            if let Err(error) = JsFuture::from(promise).await {
                if let Some(inner) = weak.upgrade() {
                    inner.borrow_mut().core.report_error(format!(
                        "could not open Web MIDI {kind} {endpoint_id}: {error:?}"
                    ));
                }
            }
        });
    }

    fn endpoint_name(port: &MidiPort) -> String {
        match (port.manufacturer(), port.name()) {
            (Some(manufacturer), Some(name)) if !manufacturer.is_empty() => {
                format!("{manufacturer}: {name}")
            }
            (_, Some(name)) => name,
            (Some(manufacturer), None) => manufacturer,
            (None, None) => port.id(),
        }
    }

    fn endpoint_id(direction: MidiEndpointDirection, raw_id: &str) -> String {
        let kind = match direction {
            MidiEndpointDirection::Input => "sink",
            MidiEndpointDirection::Output => "source",
        };
        format!("webmidi:{kind}:{raw_id}")
    }

    fn midi_supported() -> bool {
        web_sys::window().is_some_and(|window| {
            Reflect::has(
                window.navigator().as_ref(),
                &JsValue::from_str("requestMIDIAccess"),
            )
            .unwrap_or(false)
        })
    }

    #[derive(Clone)]
    pub struct BrowserMidiHub {
        inner: Rc<RefCell<HubInner>>,
    }

    impl BrowserMidiHub {
        fn new() -> Self {
            Self {
                inner: Rc::new(RefCell::new(HubInner {
                    core: BrowserMidiCore::new(midi_supported()),
                    access: None,
                    inputs: BTreeMap::new(),
                    outputs: BTreeMap::new(),
                    state_handler: None,
                })),
            }
        }

        pub fn state(&self) -> BrowserMidiState {
            self.inner.borrow().core.state()
        }

        pub fn error(&self) -> Option<String> {
            self.inner.borrow().core.error().map(str::to_owned)
        }

        pub fn sysex_enabled(&self) -> bool {
            self.inner.borrow().core.sysex_enabled()
        }

        pub fn endpoint_snapshot(&self) -> MidiEndpointSnapshot {
            self.inner.borrow().core.endpoint_snapshot()
        }

        pub fn endpoints(&self) -> Vec<BrowserMidiEndpoint> {
            self.inner.borrow().core.endpoints()
        }

        pub fn drain_track_messages(&self, max_messages: usize) -> Vec<TrackMidiInput> {
            self.inner
                .borrow_mut()
                .core
                .drain_track_messages(max_messages)
        }

        pub fn diagnostics(&self) -> (u32, u32, u32) {
            let state = self.inner.borrow();
            (
                state.core.dropped_track_messages(),
                state.core.refused_track_messages(),
                state.core.refused_control_messages(),
            )
        }

        pub fn request_access(&self) {
            if matches!(
                self.state(),
                BrowserMidiState::RequestingPermission | BrowserMidiState::Running
            ) {
                return;
            }
            let Some(window) = web_sys::window() else {
                self.inner.borrow_mut().core.set_state(
                    BrowserMidiState::Unsupported,
                    Some("browser window is unavailable".to_owned()),
                    false,
                );
                return;
            };
            let options = MidiOptions::new();
            options.set_sysex(true);
            let promise = match window
                .navigator()
                .request_midi_access_with_options(&options)
            {
                Ok(promise) => promise,
                Err(error) => {
                    self.inner.borrow_mut().core.set_state(
                        BrowserMidiState::Failed,
                        Some(format!("could not request Web MIDI: {error:?}")),
                        false,
                    );
                    return;
                }
            };
            self.inner.borrow_mut().core.set_state(
                BrowserMidiState::RequestingPermission,
                None,
                false,
            );
            let inner = self.inner.clone();
            spawn_local(async move {
                match JsFuture::from(promise).await {
                    Ok(access) => match access.dyn_into::<MidiAccess>() {
                        Ok(access) => {
                            let weak: Weak<RefCell<HubInner>> = Rc::downgrade(&inner);
                            let handler = Closure::wrap(Box::new(move |_event: Event| {
                                if let Some(inner) = weak.upgrade() {
                                    if let Err(error) = HubInner::refresh(&inner) {
                                        inner.borrow_mut().core.set_state(
                                            BrowserMidiState::Failed,
                                            Some(error.to_string()),
                                            false,
                                        );
                                    }
                                }
                            })
                                as Box<dyn FnMut(_)>);
                            access.set_onstatechange(Some(handler.as_ref().unchecked_ref()));
                            {
                                let mut state = inner.borrow_mut();
                                state.core.set_state(
                                    BrowserMidiState::Running,
                                    None,
                                    access.sysex_enabled(),
                                );
                                state.access = Some(access);
                                state.state_handler = Some(handler);
                            }
                            if let Err(error) = HubInner::refresh(&inner) {
                                inner.borrow_mut().core.set_state(
                                    BrowserMidiState::Failed,
                                    Some(error.to_string()),
                                    false,
                                );
                            }
                        }
                        Err(_) => inner.borrow_mut().core.set_state(
                            BrowserMidiState::Failed,
                            Some("browser returned invalid Web MIDI access".to_owned()),
                            false,
                        ),
                    },
                    Err(error) => inner.borrow_mut().core.set_state(
                        BrowserMidiState::Denied,
                        Some(format!("Web MIDI permission denied: {error:?}")),
                        false,
                    ),
                }
            });
        }

        fn subscribe_input(&self, endpoint_id: &str) -> anyhow::Result<u64> {
            self.inner.borrow_mut().core.subscribe_input(endpoint_id)
        }

        fn unsubscribe_input(&self, subscription: u64) {
            self.inner.borrow_mut().core.unsubscribe_input(subscription);
        }

        fn drain_subscription(
            &self,
            subscription: u64,
            max_messages: usize,
        ) -> anyhow::Result<Vec<Vec<u8>>> {
            self.inner
                .borrow_mut()
                .core
                .drain_subscription(subscription, max_messages)
        }

        fn take_subscription_dropped(&self, subscription: u64) -> u32 {
            self.inner
                .borrow_mut()
                .core
                .take_subscription_dropped(subscription)
        }

        pub fn send(&self, endpoint_id: &str, message: &[u8]) -> anyhow::Result<()> {
            if message.is_empty() || message.len() > MAX_MIDI_MESSAGE_BYTES {
                anyhow::bail!("invalid MIDI message length {}", message.len());
            }
            let output = self.inner.borrow().outputs.get(endpoint_id).cloned();
            let result = match output {
                Some(output) => {
                    let data = Uint8Array::from(message);
                    output
                        .send(data.as_ref())
                        .map_err(|error| anyhow!("could not send Web MIDI: {error:?}"))
                }
                None => Err(anyhow!(
                    "Web MIDI output endpoint disappeared: {endpoint_id}"
                )),
            };
            if let Err(error) = &result {
                self.inner.borrow_mut().core.report_error(error.to_string());
            }
            result
        }
    }

    enum ControlConnection {
        Input(u64),
        Output(String),
    }

    pub struct WebMidiControlService {
        hub: BrowserMidiHub,
        next_connection: u64,
        connections: BTreeMap<MidiConnectionId, ControlConnection>,
    }

    impl WebMidiControlService {
        fn new(hub: BrowserMidiHub) -> Self {
            Self {
                hub,
                next_connection: 1,
                connections: BTreeMap::new(),
            }
        }

        fn next_id(&mut self) -> MidiConnectionId {
            let id = MidiConnectionId::from_raw(self.next_connection);
            self.next_connection = self.next_connection.saturating_add(1);
            id
        }
    }

    impl MidiControlService for WebMidiControlService {
        fn endpoints(&mut self) -> anyhow::Result<MidiEndpointSnapshot> {
            Ok(self.hub.endpoint_snapshot())
        }

        fn connect_input(&mut self, endpoint_id: &str) -> anyhow::Result<MidiConnectionId> {
            let subscription = self.hub.subscribe_input(endpoint_id)?;
            let id = self.next_id();
            self.connections
                .insert(id, ControlConnection::Input(subscription));
            Ok(id)
        }

        fn connect_output(&mut self, endpoint_id: &str) -> anyhow::Result<MidiConnectionId> {
            let endpoint = self
                .hub
                .inner
                .borrow()
                .core
                .endpoint(endpoint_id)
                .cloned()
                .ok_or_else(|| anyhow!("Web MIDI output endpoint disappeared: {endpoint_id}"))?;
            if endpoint.direction != MidiEndpointDirection::Input {
                anyhow::bail!("Web MIDI endpoint is not an output sink: {endpoint_id}");
            }
            let id = self.next_id();
            self.connections
                .insert(id, ControlConnection::Output(endpoint_id.to_owned()));
            Ok(id)
        }

        fn disconnect(&mut self, connection: MidiConnectionId) {
            if let Some(ControlConnection::Input(subscription)) =
                self.connections.remove(&connection)
            {
                self.hub.unsubscribe_input(subscription);
            }
        }

        fn drain_input(
            &mut self,
            connection: MidiConnectionId,
            max_messages: usize,
        ) -> anyhow::Result<Vec<Vec<u8>>> {
            let Some(ControlConnection::Input(subscription)) = self.connections.get(&connection)
            else {
                anyhow::bail!("unknown Web MIDI input connection");
            };
            self.hub.drain_subscription(*subscription, max_messages)
        }

        fn take_dropped_input(&mut self, connection: MidiConnectionId) -> u32 {
            match self.connections.get(&connection) {
                Some(ControlConnection::Input(subscription)) => {
                    self.hub.take_subscription_dropped(*subscription)
                }
                _ => 0,
            }
        }

        fn send(&mut self, connection: MidiConnectionId, message: &[u8]) -> anyhow::Result<()> {
            let Some(ControlConnection::Output(endpoint_id)) = self.connections.get(&connection)
            else {
                anyhow::bail!("unknown Web MIDI output connection");
            };
            self.hub.send(endpoint_id, message)
        }
    }

    pub struct BrowserMidiController {
        hub: BrowserMidiHub,
        _enable_handler: Closure<dyn FnMut(Event)>,
    }

    impl BrowserMidiController {
        pub fn new() -> anyhow::Result<(Self, Box<dyn MidiControlService>)> {
            let hub = BrowserMidiHub::new();
            let button = web_sys::window()
                .and_then(|window| window.document())
                .and_then(|document| document.get_element_by_id(ENABLE_BUTTON_ID))
                .ok_or_else(|| anyhow!("missing Web MIDI enable button"))?
                .dyn_into::<HtmlButtonElement>()
                .map_err(|_| anyhow!("Web MIDI enable element is not a button"))?;
            let handler_hub = hub.clone();
            let enable_handler = Closure::wrap(Box::new(move |_event: Event| {
                handler_hub.request_access();
            }) as Box<dyn FnMut(_)>);
            button.set_onclick(Some(enable_handler.as_ref().unchecked_ref()));
            Ok((
                Self {
                    hub: hub.clone(),
                    _enable_handler: enable_handler,
                },
                Box::new(WebMidiControlService::new(hub)),
            ))
        }

        pub fn hub(&self) -> BrowserMidiHub {
            self.hub.clone()
        }

        pub fn state(&self) -> BrowserMidiState {
            self.hub.state()
        }

        pub fn endpoint_count(&self) -> usize {
            self.hub.endpoints().len()
        }

        pub fn diagnostics(&self) -> (u32, u32, u32) {
            self.hub.diagnostics()
        }

        pub fn update_presentation(&self) {
            let Some(button) = web_sys::window()
                .and_then(|window| window.document())
                .and_then(|document| document.get_element_by_id(ENABLE_BUTTON_ID))
                .and_then(|element| element.dyn_into::<HtmlButtonElement>().ok())
            else {
                return;
            };
            match self.hub.state() {
                BrowserMidiState::Unsupported => {
                    button.set_hidden(false);
                    button.set_disabled(true);
                    button.set_text_content(Some("Web MIDI unsupported"));
                }
                BrowserMidiState::AwaitingGesture => {
                    button.set_hidden(false);
                    button.set_disabled(false);
                    button.set_text_content(Some("Enable Web MIDI + SysEx"));
                }
                BrowserMidiState::RequestingPermission => {
                    button.set_hidden(false);
                    button.set_disabled(true);
                    button.set_text_content(Some("Requesting Web MIDI…"));
                }
                BrowserMidiState::Running => {
                    button.set_hidden(false);
                    button.set_disabled(true);
                    let suffix = if self.hub.sysex_enabled() {
                        "SysEx enabled"
                    } else {
                        "SysEx unavailable"
                    };
                    button.set_text_content(Some(&format!("Web MIDI enabled ({suffix})")));
                }
                BrowserMidiState::Denied | BrowserMidiState::Failed => {
                    button.set_hidden(false);
                    button.set_disabled(false);
                    button.set_text_content(Some("Retry Web MIDI + SysEx"));
                }
            }
            let (dropped, refused_track, refused_control) = self.hub.diagnostics();
            let diagnostics = format!(
                "{}; track drops: {dropped}; track refusals: {refused_track}; control refusals: {refused_control}",
                self.hub.error().as_deref().unwrap_or("Web MIDI ready")
            );
            button.set_title(&diagnostics);
        }
    }

    impl Drop for BrowserMidiController {
        fn drop(&mut self) {
            if let Some(access) = self.hub.inner.borrow().access.as_ref() {
                access.set_onstatechange(None);
            }
            let state = self.hub.inner.borrow();
            for handle in state.inputs.values() {
                handle.port.set_onmidimessage(None);
                let port: &MidiPort = handle.port.unchecked_ref();
                let _ = port.close();
            }
            for output in state.outputs.values() {
                let port: &MidiPort = output.unchecked_ref();
                let _ = port.close();
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use platform::{BrowserMidiController, BrowserMidiHub};

#[cfg(test)]
mod tests {
    use super::*;

    fn source(id: &str) -> BrowserMidiEndpoint {
        BrowserMidiEndpoint {
            id: format!("webmidi:source:{id}"),
            raw_id: id.to_owned(),
            name: format!("Controller {id}"),
            direction: MidiEndpointDirection::Output,
        }
    }

    fn sink(id: &str) -> BrowserMidiEndpoint {
        BrowserMidiEndpoint {
            id: format!("webmidi:sink:{id}"),
            raw_id: id.to_owned(),
            name: format!("Controller {id}"),
            direction: MidiEndpointDirection::Input,
        }
    }

    #[test]
    fn lifecycle_and_hotplug_publish_revisioned_stable_endpoints() {
        let mut core = BrowserMidiCore::new(true);
        assert_eq!(core.state(), BrowserMidiState::AwaitingGesture);
        core.set_state(BrowserMidiState::RequestingPermission, None, false);
        assert_eq!(core.state(), BrowserMidiState::RequestingPermission);
        core.set_state(BrowserMidiState::Failed, Some("retry".to_owned()), false);
        assert_eq!(core.state(), BrowserMidiState::Failed);
        core.set_state(BrowserMidiState::Running, None, true);
        assert!(core.sysex_enabled());
        core.report_error("open failed".to_owned());
        assert_eq!(core.error(), Some("open failed"));
        core.replace_endpoints(vec![sink("out"), source("in")]);
        let first = core.endpoint_snapshot();
        assert_eq!(first.revision, 1);
        assert_eq!(first.endpoints.len(), 2);
        assert_eq!(core.endpoints().len(), 2);
        assert_eq!(
            core.endpoint("webmidi:source:in")
                .map(|endpoint| endpoint.raw_id.as_str()),
            Some("in")
        );
        core.replace_endpoints(vec![source("in"), sink("out")]);
        assert_eq!(core.endpoint_snapshot().revision, 1);
        core.replace_endpoints(vec![source("in")]);
        assert_eq!(core.endpoint_snapshot().revision, 2);
        core.set_state(
            BrowserMidiState::Denied,
            Some("permission denied".to_owned()),
            false,
        );
        assert!(core.endpoint_snapshot().endpoints.is_empty());
        assert_eq!(core.error(), Some("permission denied"));
    }

    #[test]
    fn input_fans_out_to_control_subscribers_and_track_queue() {
        let mut core = BrowserMidiCore::new(true);
        core.set_state(BrowserMidiState::Running, None, false);
        let endpoint = source("in");
        let endpoint_id = endpoint.id.clone();
        core.replace_endpoints(vec![endpoint]);
        let one = core.subscribe_input(&endpoint_id).unwrap();
        let two = core.subscribe_input(&endpoint_id).unwrap();
        let removed = core.subscribe_input(&endpoint_id).unwrap();
        core.unsubscribe_input(removed);
        core.receive(&endpoint_id, &[0x90, 60, 100]);
        assert_eq!(
            core.drain_subscription(one, 1).unwrap(),
            vec![vec![0x90, 60, 100]]
        );
        assert_eq!(
            core.drain_subscription(two, 1).unwrap(),
            vec![vec![0x90, 60, 100]]
        );
        assert_eq!(
            core.drain_track_messages(1),
            vec![TrackMidiInput {
                endpoint_id,
                data: vec![0x90, 60, 100],
            }]
        );
    }

    #[test]
    fn track_and_control_limits_refuse_without_truncation() {
        let mut core = BrowserMidiCore::new(true);
        core.set_state(BrowserMidiState::Running, None, true);
        let endpoint = source("in");
        let endpoint_id = endpoint.id.clone();
        core.replace_endpoints(vec![endpoint]);
        let subscription = core.subscribe_input(&endpoint_id).unwrap();
        let five_bytes = vec![0xf0, 1, 2, 3, 0xf7];
        core.receive(&endpoint_id, &five_bytes);
        assert_eq!(
            core.drain_subscription(subscription, 1).unwrap(),
            vec![five_bytes]
        );
        assert!(core.drain_track_messages(1).is_empty());
        assert_eq!(core.refused_track_messages(), 1);

        core.receive(&endpoint_id, &vec![0; MAX_MIDI_MESSAGE_BYTES + 1]);
        assert_eq!(core.refused_control_messages(), 1);
        assert_eq!(core.refused_track_messages(), 2);
    }

    #[test]
    fn bounded_queues_count_drops() {
        let mut core = BrowserMidiCore::new(true);
        core.set_state(BrowserMidiState::Running, None, false);
        let endpoint = source("in");
        let endpoint_id = endpoint.id.clone();
        core.replace_endpoints(vec![endpoint]);
        let subscription = core.subscribe_input(&endpoint_id).unwrap();
        for note in 0..=MIDI_INPUT_QUEUE_CAPACITY {
            core.receive(&endpoint_id, &[0x90, note as u8, 100]);
        }
        assert_eq!(
            core.drain_subscription(subscription, usize::MAX)
                .unwrap()
                .len(),
            MIDI_INPUT_QUEUE_CAPACITY
        );
        assert_eq!(core.take_subscription_dropped(subscription), 1);
        assert_eq!(
            core.drain_track_messages(usize::MAX).len(),
            MIDI_TRACK_QUEUE_CAPACITY
        );
        assert_eq!(core.dropped_track_messages(), 1);
    }

    #[test]
    fn subscription_capacity_is_bounded() {
        let mut core = BrowserMidiCore::new(true);
        core.set_state(BrowserMidiState::Running, None, false);
        let endpoint = source("bounded");
        let endpoint_id = endpoint.id.clone();
        core.replace_endpoints(vec![endpoint]);
        for _ in 0..MIDI_SUBSCRIPTION_CAPACITY {
            core.subscribe_input(&endpoint_id).unwrap();
        }
        assert!(core.subscribe_input(&endpoint_id).is_err());
    }

    #[test]
    fn direction_validation_rejects_output_as_input_source() {
        let mut core = BrowserMidiCore::new(true);
        core.set_state(BrowserMidiState::Running, None, false);
        let endpoint = sink("out");
        let endpoint_id = endpoint.id.clone();
        core.replace_endpoints(vec![endpoint]);
        assert!(core.subscribe_input(&endpoint_id).is_err());
    }
}
