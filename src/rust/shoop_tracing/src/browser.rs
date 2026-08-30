use std::cell::RefCell;
use std::fmt::Write as _;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;

use base64::Engine as _;
use perfetto_everywhere_collector::{Collector, CollectorConfig, RealmDescriptor};
use perfetto_everywhere_core::{MetadataId, StaticName, TraceBackend, TrackId};
use perfetto_everywhere_tracing::{PerfettoLayer, SharedBackend};
use perfetto_everywhere_web::{
    ClockCalibration, MetadataEntry, OrdinaryBackend, PerformanceClock, ProducerHealth,
};
use tracing::{field::Visit, Event, Subscriber};
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Layer;

const WINDOW_REALM_ID: u32 = 1;
const WINDOW_CLOCK_ID: u32 = 101;
const WINDOW_TICKS_PER_SECOND: u64 = 1_000_000_000;
const WINDOW_BATCH_RECORDS: usize = 16_384;

static BROWSER_CAPTURE_ENABLED: AtomicBool = AtomicBool::new(false);
static WASM_TEST_PANIC_HOOK: Once = Once::new();

struct BrowserState {
    backend: SharedBackend<OrdinaryBackend<PerformanceClock>>,
    records: Vec<u8>,
    active: bool,
}

struct ConsoleLayer;

struct ConsoleVisitor<'a>(&'a mut String);

impl Visit for ConsoleVisitor<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let _ = write!(self.0, " {}={value:?}", field.name());
    }
}

impl<S: Subscriber> Layer<S> for ConsoleLayer {
    fn on_event(&self, event: &Event<'_>, _context: tracing_subscriber::layer::Context<'_, S>) {
        let metadata = event.metadata();
        let mut text = format!("[{}] [{}]", metadata.level(), metadata.target());
        event.record(&mut ConsoleVisitor(&mut text));
        let value = wasm_bindgen::JsValue::from_str(&text);
        match *metadata.level() {
            tracing::Level::ERROR => web_sys::console::error_1(&value),
            tracing::Level::WARN => web_sys::console::warn_1(&value),
            tracing::Level::INFO => web_sys::console::info_1(&value),
            tracing::Level::DEBUG => web_sys::console::debug_1(&value),
            tracing::Level::TRACE => web_sys::console::log_1(&value),
        }
    }
}

thread_local! {
    static STATE: RefCell<Option<BrowserState>> = const { RefCell::new(None) };
    static TEST_CAPTURE: RefCell<Option<(String, BrowserCapture, BrowserCalibration, bool)>> =
        const { RefCell::new(None) };
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserMetadata {
    pub id: u32,
    pub namespace: u8,
    pub label: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrowserCalibration {
    pub realm_id: u32,
    pub clock_id: u32,
    pub source_ticks: u64,
    pub reference_time_ns: u64,
    pub uncertainty_ns: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BrowserHealth {
    pub emitted_records: u64,
    pub dropped_records: u64,
    pub completed_batches: u64,
    pub high_water_records: usize,
    pub repaired_span_boundaries: u64,
}

#[derive(Clone, Debug)]
pub struct BrowserRealmData {
    pub id: u32,
    pub label: String,
    pub ticks_per_second: u64,
    pub records: Vec<u8>,
    pub metadata: Vec<BrowserMetadata>,
    pub calibrations: Vec<BrowserCalibration>,
    pub health: BrowserHealth,
}

pub struct BrowserCapture {
    finished: bool,
    not_send: PhantomData<Rc<()>>,
}

pub fn initialize_browser_tracing() -> Result<(), String> {
    STATE.with(|state| {
        if state.borrow().is_some() {
            return Ok(());
        }
        let backend = OrdinaryBackend::new(
            WINDOW_REALM_ID,
            WINDOW_CLOCK_ID,
            PerformanceClock,
            WINDOW_BATCH_RECORDS,
            &[],
        )?;
        backend.set_enabled(false);
        let layer = PerfettoLayer::new(backend);
        let backend = layer.backend_handle();
        let filtered = layer.with_filter(filter_fn(|metadata| {
            (metadata.is_span() || metadata.is_event())
                && BROWSER_CAPTURE_ENABLED.load(Ordering::Acquire)
        }));
        let subscriber = tracing_subscriber::registry()
            .with(filtered)
            .with(ConsoleLayer);
        tracing::subscriber::set_global_default(subscriber)
            .map_err(|error| format!("could not install browser tracing subscriber: {error}"))?;
        let _ = tracing_log::LogTracer::init();
        *state.borrow_mut() = Some(BrowserState {
            backend,
            records: Vec::new(),
            active: false,
        });
        Ok(())
    })
}

impl BrowserCapture {
    pub fn start(engine_detail: bool) -> Result<Self, String> {
        initialize_browser_tracing()?;
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            let state = state
                .as_mut()
                .ok_or_else(|| "browser tracing state is unavailable".to_owned())?;
            if state.active {
                return Err("browser tracing is already active".to_owned());
            }
            state.records.clear();
            state
                .backend
                .with(|backend| {
                    backend.set_enabled(true);
                    clear_backend(backend);
                })
                .ok_or_else(|| "browser tracing backend lock is poisoned".to_owned())?;
            state.active = true;
            let _ = engine_detail;
            BROWSER_CAPTURE_ENABLED.store(true, Ordering::Release);
            Ok(Self {
                finished: false,
                not_send: PhantomData,
            })
        })
    }

    pub fn poll(&self) -> Result<(), String> {
        if self.finished {
            return Ok(());
        }
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            let state = state
                .as_mut()
                .ok_or_else(|| "browser tracing state is unavailable".to_owned())?;
            drain(state)
        })
    }

    pub fn finish(
        mut self,
        window_calibrations: Vec<BrowserCalibration>,
        mut realms: Vec<BrowserRealmData>,
    ) -> Result<Vec<u8>, String> {
        let window = STATE.with(|state| {
            let mut state = state.borrow_mut();
            let state = state
                .as_mut()
                .ok_or_else(|| "browser tracing state is unavailable".to_owned())?;
            if !state.active {
                return Err("browser tracing is not active".to_owned());
            }
            BROWSER_CAPTURE_ENABLED.store(false, Ordering::Release);
            state
                .backend
                .with(|backend| backend.set_enabled(false))
                .ok_or_else(|| "browser tracing backend lock is poisoned".to_owned())?;
            drain(state)?;
            let (metadata, health) = state
                .backend
                .with(|backend| (backend.take_metadata(), backend.health()))
                .ok_or_else(|| "browser tracing backend lock is poisoned".to_owned())?;
            state.active = false;
            Ok(BrowserRealmData {
                id: WINDOW_REALM_ID,
                label: "Window".to_owned(),
                ticks_per_second: WINDOW_TICKS_PER_SECOND,
                records: std::mem::take(&mut state.records),
                metadata: metadata.into_iter().map(metadata_from_perfetto).collect(),
                calibrations: window_calibrations,
                health: health_from_perfetto(health),
            })
        })?;
        self.finished = true;
        realms.insert(0, window);
        collect(realms)
    }

    pub fn discard(mut self) -> Result<(), String> {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            let state = state
                .as_mut()
                .ok_or_else(|| "browser tracing state is unavailable".to_owned())?;
            BROWSER_CAPTURE_ENABLED.store(false, Ordering::Release);
            state
                .backend
                .with(|backend| {
                    backend.set_enabled(false);
                    clear_backend(backend);
                })
                .ok_or_else(|| "browser tracing backend lock is poisoned".to_owned())?;
            state.records.clear();
            state.active = false;
            Ok::<(), String>(())
        })?;
        self.finished = true;
        Ok(())
    }
}

impl Drop for BrowserCapture {
    fn drop(&mut self) {
        if !self.finished {
            BROWSER_CAPTURE_ENABLED.store(false, Ordering::Release);
            STATE.with(|state| {
                if let Some(state) = state.borrow_mut().as_mut() {
                    let _ = state.backend.with(|backend| backend.set_enabled(false));
                    state.records.clear();
                    state.active = false;
                }
            });
        }
    }
}

fn clear_backend(backend: &OrdinaryBackend<PerformanceClock>) {
    let _ = backend.take_metadata();
    loop {
        if backend.take_batch().is_some() {
            continue;
        }
        if backend.flush_and_take_batch().is_none() {
            break;
        }
    }
}

fn drain(state: &mut BrowserState) -> Result<(), String> {
    loop {
        let batch = state
            .backend
            .with(|backend| {
                if let Some(batch) = backend.take_batch() {
                    return Some(batch);
                }
                backend.flush_and_take_batch()
            })
            .ok_or_else(|| "browser tracing backend lock is poisoned".to_owned())?;
        let Some(batch) = batch else {
            break;
        };
        state.records.extend_from_slice(&batch);
    }
    Ok(())
}

fn collect(realms: Vec<BrowserRealmData>) -> Result<Vec<u8>, String> {
    let mut config = CollectorConfig::default();
    config.max_clock_uncertainty_ns = 1_000_000_000;
    let mut collector = Collector::new(config);
    for realm in realms {
        collector
            .register_realm(RealmDescriptor {
                id: realm.id,
                label: realm.label,
                ticks_per_second: realm.ticks_per_second,
            })
            .map_err(|error| error.to_string())?;
        collector
            .register_metadata_all(realm.metadata.into_iter().map(metadata_to_perfetto))
            .map_err(|error| error.to_string())?;
        for calibration in realm.calibrations {
            collector
                .add_calibration(ClockCalibration {
                    realm_id: calibration.realm_id,
                    clock_id: calibration.clock_id,
                    source_ticks: calibration.source_ticks,
                    reference_time_ns: calibration.reference_time_ns,
                    uncertainty_ns: calibration.uncertainty_ns,
                })
                .map_err(|error| error.to_string())?;
        }
        collector
            .ingest_batch(&realm.records)
            .map_err(|error| error.to_string())?;
        collector.set_health(realm.id, health_to_perfetto(realm.health));
    }
    collector.finish().map_err(|error| error.to_string())
}

fn metadata_from_perfetto(metadata: MetadataEntry) -> BrowserMetadata {
    BrowserMetadata {
        id: metadata.id.0,
        namespace: metadata.namespace,
        label: metadata.label,
    }
}

fn metadata_to_perfetto(metadata: BrowserMetadata) -> MetadataEntry {
    MetadataEntry {
        id: MetadataId(metadata.id),
        namespace: metadata.namespace,
        label: metadata.label,
    }
}

fn health_from_perfetto(health: ProducerHealth) -> BrowserHealth {
    BrowserHealth {
        emitted_records: health.emitted_records,
        dropped_records: health.dropped_records,
        completed_batches: health.completed_batches,
        high_water_records: health.high_water_records,
        repaired_span_boundaries: health.repaired_span_boundaries,
    }
}

fn health_to_perfetto(health: BrowserHealth) -> ProducerHealth {
    ProducerHealth {
        emitted_records: health.emitted_records,
        dropped_records: health.dropped_records,
        completed_batches: health.completed_batches,
        high_water_records: health.high_water_records,
        repaired_span_boundaries: health.repaired_span_boundaries,
    }
}

pub fn wasm_test_trace_begin(module: &str, test: &str, panic_expected: bool) {
    if !wasm_test_tracing_enabled() {
        return;
    }
    let identity = format!("{module}::{test}");
    initialize_browser_tracing().expect("initialize Wasm test tracing");
    install_wasm_test_panic_hook();
    if let Some((_identity, stale, _calibration, _panic_expected)) =
        TEST_CAPTURE.with(|slot| slot.borrow_mut().take())
    {
        stale
            .discard()
            .expect("discard trace left by a trapped Wasm testcase");
    }

    let bootstrap = BrowserCapture::start(false).expect("start Wasm failure bootstrap trace");
    let bootstrap_start = ordinary_calibration();
    emit_test_markers(&identity, "bootstrap");
    bootstrap.poll().expect("poll Wasm failure bootstrap trace");
    let bytes = bootstrap
        .finish(vec![bootstrap_start, ordinary_calibration()], Vec::new())
        .expect("finish Wasm failure bootstrap trace");
    emit_test_trace(&identity, "bootstrap", &bytes);

    let capture = BrowserCapture::start(false).expect("start Wasm testcase trace");
    let capture_start = ordinary_calibration();
    emit_test_markers(&identity, "begin");
    TEST_CAPTURE.with(|slot| {
        let previous =
            slot.borrow_mut()
                .replace((identity, capture, capture_start, panic_expected));
        assert!(previous.is_none(), "Wasm testcase traces must not overlap");
    });
}

pub fn wasm_test_trace_finish() {
    if !wasm_test_tracing_enabled() {
        return;
    }
    finalize_wasm_test_trace("success").expect("finish Wasm testcase trace");
}

pub fn wasm_test_trace_finish_result(failed: bool) {
    if !wasm_test_tracing_enabled() {
        return;
    }
    finalize_wasm_test_trace(if failed { "failure" } else { "success" })
        .expect("finish Wasm Result testcase trace");
}

fn install_wasm_test_panic_hook() {
    WASM_TEST_PANIC_HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |information| {
            let phase = TEST_CAPTURE.with(|slot| {
                slot.borrow().as_ref().map(|(_, _, _, panic_expected)| {
                    if *panic_expected {
                        "expected-panic"
                    } else {
                        "failure"
                    }
                })
            });
            if let Some(phase) = phase {
                if let Err(error) = finalize_wasm_test_trace(phase) {
                    web_sys::console::error_1(
                        &format!("could not finalize panicking Wasm testcase trace: {error}")
                            .into(),
                    );
                }
            }
            previous(information);
        }));
    });
}

fn finalize_wasm_test_trace(phase: &str) -> Result<(), String> {
    let capture = TEST_CAPTURE.with(|slot| slot.borrow_mut().take());
    let Some((identity, capture, capture_start, _panic_expected)) = capture else {
        return Err("Wasm testcase trace is not active".to_owned());
    };
    emit_test_markers(&identity, phase);
    capture.poll()?;
    let bytes = capture.finish(vec![capture_start, try_ordinary_calibration()?], Vec::new())?;
    emit_test_trace(&identity, "full", &bytes);
    Ok(())
}

fn wasm_test_tracing_enabled() -> bool {
    !matches!(option_env!("SHOOP_WASM_TEST_TRACE"), None | Some("off"))
}

fn emit_test_markers(identity: &str, phase: &str) {
    {
        let span = tracing::info_span!(
            "shoop.wasm_test_capture.span",
            test = %identity,
            phase,
        );
        let _entered = span.enter();
        tracing::info!(test = %identity, phase, "shoop.wasm_test_capture.event");
        log::info!("shoop.wasm_test_capture.log test={identity} phase={phase}");
    }
    STATE.with(|state| {
        if let Some(state) = state.borrow().as_ref() {
            let _ = state.backend.counter_i64(
                StaticName::new("shoop.wasm_test_capture.plot"),
                TrackId::CURRENT,
                1,
            );
        }
    });
}

fn ordinary_calibration() -> BrowserCalibration {
    try_ordinary_calibration().expect("performance clock for Wasm test tracing")
}

fn try_ordinary_calibration() -> Result<BrowserCalibration, String> {
    use wasm_bindgen::JsCast as _;

    let global = js_sys::global();
    let performance = js_sys::Reflect::get(&global, &"performance".into())
        .map_err(|error| format!("performance object is unavailable: {error:?}"))?;
    let now = js_sys::Reflect::get(&performance, &"now".into())
        .map_err(|error| format!("performance.now is unavailable: {error:?}"))?
        .dyn_into::<js_sys::Function>()
        .map_err(|_| "performance.now is not a function".to_owned())?;
    let time_origin = js_sys::Reflect::get(&performance, &"timeOrigin".into())
        .map_err(|error| format!("performance.timeOrigin is unavailable: {error:?}"))?
        .as_f64()
        .ok_or_else(|| "performance.timeOrigin is not numeric".to_owned())?;
    let before = now
        .call0(&performance)
        .map_err(|error| format!("first performance.now sample failed: {error:?}"))?
        .as_f64()
        .ok_or_else(|| "first performance.now sample is not numeric".to_owned())?;
    let after = now
        .call0(&performance)
        .map_err(|error| format!("second performance.now sample failed: {error:?}"))?
        .as_f64()
        .ok_or_else(|| "second performance.now sample is not numeric".to_owned())?;
    let source_ms = (before + after) * 0.5;
    Ok(BrowserCalibration {
        realm_id: WINDOW_REALM_ID,
        clock_id: WINDOW_CLOCK_ID,
        source_ticks: (source_ms * 1_000_000.0).round() as u64,
        reference_time_ns: ((time_origin + source_ms) * 1_000_000.0).round() as u64,
        uncertainty_ns: (((after - before) * 500_000.0).round() as u64).max(1),
    })
}

#[wasm_bindgen::prelude::wasm_bindgen(inline_js = r#"
export function shoopWriteTestTrace(identity, phase, trace, sink) {
  if (globalThis.process?.versions?.node) {
    const directory = process.env.SHOOP_WASM_TEST_TRACE_DIR;
    if (!directory) throw new Error("SHOOP_WASM_TEST_TRACE_DIR is missing");
    const fs = process.getBuiltinModule("fs");
    fs.mkdirSync(directory, {recursive: true});
    fs.writeFileSync(`${directory}/${identity}.${phase}.pftrace`, trace, "base64");
    return;
  }
  if (!sink) throw new Error("SHOOP_WASM_TEST_ASSET_BASE is missing");
  const request = new XMLHttpRequest();
  request.open("POST", `${sink}/__shoop_trace/${identity}/${phase}`, false);
  request.setRequestHeader("Content-Type", "text/plain");
  request.send(trace);
  if (request.status !== 204) {
    throw new Error(`trace sink rejected ${identity}/${phase}: ${request.status}`);
  }
}
"#)]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(js_name = shoopWriteTestTrace)]
    fn write_test_trace(identity: &str, phase: &str, trace: &str, sink: &str);
}

fn emit_test_trace(identity: &str, phase: &str, bytes: &[u8]) {
    let identity = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(identity);
    let trace = base64::engine::general_purpose::STANDARD.encode(bytes);
    write_test_trace(
        &identity,
        phase,
        &trace,
        option_env!("SHOOP_WASM_TEST_ASSET_BASE").unwrap_or(""),
    );
}
