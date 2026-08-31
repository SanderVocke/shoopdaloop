use anyhow::anyhow;
use js_sys::{Array, ArrayBuffer, Object, Reflect, Uint8Array};
use std::cell::Cell;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::MessagePort;

const RECORD_BYTES: usize = 48;
const TRACE_PROTOCOL_VERSION: u32 = 2;
const TRACE_POOL_SIZE: usize = 3;
const MAX_SAFE_INTEGER: u64 = (1_u64 << 53) - 1;
const MAX_REALM_TRACE_BYTES: usize = 512 * 1024 * 1024;

thread_local! {
    static NEXT_CAPTURE_ID: Cell<u64> = const { Cell::new(1) };
}

type Result<T> = std::result::Result<T, BrowserTraceError>;

#[derive(Debug)]
pub struct BrowserTraceError(String);

impl std::fmt::Display for BrowserTraceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for BrowserTraceError {}

impl From<JsValue> for BrowserTraceError {
    fn from(value: JsValue) -> Self {
        Self(format!("{value:?}"))
    }
}

impl From<anyhow::Error> for BrowserTraceError {
    fn from(value: anyhow::Error) -> Self {
        Self(value.to_string())
    }
}

fn reference_time_ms() -> Result<f64> {
    let performance = web_sys::window()
        .ok_or_else(|| anyhow!("browser window is unavailable"))?
        .performance()
        .ok_or_else(|| anyhow!("browser performance clock is unavailable"))?;
    Ok(performance.time_origin() + performance.now())
}

fn number(data: &JsValue, name: &str) -> Result<f64> {
    Reflect::get(data, &name.into())?
        .as_f64()
        .ok_or_else(|| anyhow!("trace message {name} is not numeric").into())
}

fn integer(data: &JsValue, name: &str, maximum: u64) -> Result<u64> {
    let value = number(data, name)?;
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > maximum as f64 {
        return Err(anyhow!("trace message {name} is not a valid integer").into());
    }
    Ok(value as u64)
}

fn next_capture_id() -> u64 {
    NEXT_CAPTURE_ID.with(|next| {
        let id = next.get();
        next.set(if id == MAX_SAFE_INTEGER { 1 } else { id + 1 });
        id
    })
}

#[derive(Debug)]
enum TerminalState {
    Active,
    Complete,
    Aborted(String),
}

pub struct RealmTraceState {
    realm_id: u32,
    capture_id: u64,
    clock_id: u32,
    label: String,
    ticks_per_second: u64,
    chunk_bytes: usize,
    next_sequence: u64,
    collector_owned_tokens: Vec<bool>,
    metadata: Vec<shoop_tracing::BrowserMetadata>,
    calibrations: Vec<shoop_tracing::BrowserCalibration>,
    records: Vec<u8>,
    health: shoop_tracing::BrowserHealth,
    terminal: TerminalState,
}

impl RealmTraceState {
    pub fn new(
        realm_id: u32,
        clock_id: u32,
        label: impl Into<String>,
        ticks_per_second: u64,
        _quantum_frames: u32,
        capacity: u32,
    ) -> Result<Self> {
        if realm_id == 0 || clock_id == 0 || capacity == 0 {
            return Err(anyhow!("trace realm, clock, and capacity must be nonzero").into());
        }
        Ok(Self {
            realm_id,
            capture_id: next_capture_id(),
            clock_id,
            label: label.into(),
            ticks_per_second,
            chunk_bytes: capacity as usize * RECORD_BYTES,
            next_sequence: 0,
            collector_owned_tokens: vec![false; TRACE_POOL_SIZE],
            metadata: Vec::new(),
            calibrations: Vec::new(),
            records: Vec::new(),
            health: shoop_tracing::BrowserHealth::default(),
            terminal: TerminalState::Active,
        })
    }

    pub fn start_message(&self, engine_detail: bool) -> Result<JsValue> {
        let message = Object::new();
        Reflect::set(&message, &"kind".into(), &"shoop-trace-start".into())?;
        Reflect::set(
            &message,
            &"protocolVersion".into(),
            &JsValue::from_f64(TRACE_PROTOCOL_VERSION as f64),
        )?;
        Reflect::set(
            &message,
            &"captureId".into(),
            &JsValue::from_f64(self.capture_id as f64),
        )?;
        Reflect::set(
            &message,
            &"realmId".into(),
            &JsValue::from_f64(self.realm_id as f64),
        )?;
        Reflect::set(
            &message,
            &"clockId".into(),
            &JsValue::from_f64(self.clock_id as f64),
        )?;
        Reflect::set(
            &message,
            &"capacityRecords".into(),
            &JsValue::from_f64((self.chunk_bytes / RECORD_BYTES) as f64),
        )?;
        Reflect::set(
            &message,
            &"chunkBytes".into(),
            &JsValue::from_f64(self.chunk_bytes as f64),
        )?;
        Reflect::set(
            &message,
            &"poolSize".into(),
            &JsValue::from_f64(TRACE_POOL_SIZE as f64),
        )?;
        Reflect::set(
            &message,
            &"engineDetail".into(),
            &JsValue::from_bool(engine_detail),
        )?;
        Reflect::set(
            &message,
            &"referenceMs".into(),
            &JsValue::from_f64(reference_time_ms()?),
        )?;
        Ok(message.into())
    }

    pub fn stop_message(&self) -> Result<JsValue> {
        let message = Object::new();
        Reflect::set(&message, &"kind".into(), &"shoop-trace-stop".into())?;
        Reflect::set(
            &message,
            &"captureId".into(),
            &JsValue::from_f64(self.capture_id as f64),
        )?;
        Reflect::set(
            &message,
            &"referenceMs".into(),
            &JsValue::from_f64(reference_time_ms()?),
        )?;
        Ok(message.into())
    }

    pub fn abort_message(&self) -> Result<JsValue> {
        let message = Object::new();
        Reflect::set(&message, &"kind".into(), &"shoop-trace-abort".into())?;
        Reflect::set(
            &message,
            &"captureId".into(),
            &JsValue::from_f64(self.capture_id as f64),
        )?;
        Ok(message.into())
    }

    pub fn is_trace_message(data: &JsValue) -> bool {
        Reflect::get(data, &"kind".into())
            .ok()
            .and_then(|value| value.as_string())
            .is_some_and(|kind| kind.starts_with("shoop-trace-"))
    }

    pub fn poll(&mut self) -> Result<()> {
        Ok(())
    }

    pub fn handle_message(&mut self, data: &JsValue, port: &MessagePort) -> Result<bool> {
        let kind = Reflect::get(data, &"kind".into())
            .ok()
            .and_then(|value| value.as_string());
        if kind
            .as_deref()
            .is_some_and(|kind| kind.starts_with("shoop-trace-"))
            && !self.matches_capture(data)?
        {
            tracing::debug!(
                realm = self.realm_id,
                "frontend.browser_trace.stale_message_ignored"
            );
            return Ok(true);
        }
        match kind.as_deref() {
            Some("shoop-trace-metadata") => {
                let entries = Array::from(&Reflect::get(data, &"metadata".into())?);
                self.metadata.clear();
                for value in entries.iter() {
                    self.metadata.push(shoop_tracing::BrowserMetadata {
                        id: integer(&value, "id", u32::MAX.into())? as u32,
                        namespace: integer(&value, "namespace", u8::MAX.into())? as u8,
                        label: Reflect::get(&value, &"label".into())?
                            .as_string()
                            .ok_or_else(|| anyhow!("trace metadata label is not a string"))?,
                    });
                }
                self.add_message_calibration(data)?;
                tracing::info!(
                    realm = self.realm_id,
                    "frontend.browser_trace.metadata_received"
                );
                Ok(true)
            }
            Some("shoop-trace-chunk") => {
                self.handle_chunk(data, port)?;
                Ok(true)
            }
            Some("shoop-trace-stopped") => {
                let chunk_count = integer(data, "chunkCount", MAX_SAFE_INTEGER)?;
                if chunk_count != self.next_sequence {
                    return Err(anyhow!(
                        "trace stopped with {chunk_count} chunks after receiving {}",
                        self.next_sequence
                    )
                    .into());
                }
                self.health.emitted_records = integer(data, "emittedRecords", MAX_SAFE_INTEGER)?;
                self.health.dropped_records = integer(data, "droppedRecords", MAX_SAFE_INTEGER)?;
                self.health.raw_dropped_records =
                    integer(data, "rawDroppedRecords", MAX_SAFE_INTEGER)?;
                self.health.pool_starvation_records =
                    integer(data, "poolStarvationRecords", MAX_SAFE_INTEGER)?;
                self.health.completed_batches = chunk_count;
                self.health.high_water_records =
                    integer(data, "highWaterRecords", usize::MAX as u64)? as usize;
                self.health.max_in_flight_chunks =
                    integer(data, "maxInFlight", TRACE_POOL_SIZE as u64)? as usize;
                self.health.returned_buffers = integer(data, "returnedBuffers", MAX_SAFE_INTEGER)?;
                self.health.rejected_chunks = integer(data, "rejectedChunks", MAX_SAFE_INTEGER)?;
                self.add_message_calibration(data)?;
                self.terminal = TerminalState::Complete;
                tracing::info!(realm = self.realm_id, "frontend.browser_trace.stopped");
                Ok(true)
            }
            Some("shoop-trace-aborted") => {
                self.abort_with_reason(format!("{} trace producer aborted", self.label));
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn handle_chunk(&mut self, data: &JsValue, port: &MessagePort) -> Result<()> {
        let sequence = integer(data, "sequence", MAX_SAFE_INTEGER)?;
        if sequence != self.next_sequence {
            return Err(anyhow!(
                "expected trace chunk {}, received {sequence}",
                self.next_sequence
            )
            .into());
        }
        let token = integer(data, "poolToken", (TRACE_POOL_SIZE - 1) as u64)? as usize;
        let owned = self
            .collector_owned_tokens
            .get_mut(token)
            .ok_or_else(|| anyhow!("unknown trace pool token {token}"))?;
        if *owned {
            return Err(anyhow!("trace pool token {token} is already collector-owned").into());
        }
        let used_bytes = integer(data, "usedBytes", self.chunk_bytes as u64)? as usize;
        if used_bytes == 0 || used_bytes > self.chunk_bytes || used_bytes % RECORD_BYTES != 0 {
            return Err(anyhow!("invalid trace chunk byte count {used_bytes}").into());
        }
        let buffer = Reflect::get(data, &"buffer".into())?
            .dyn_into::<ArrayBuffer>()
            .map_err(|_| anyhow!("trace chunk buffer is not an ArrayBuffer"))?;
        if buffer.byte_length() as usize != self.chunk_bytes {
            return Err(anyhow!("trace chunk capacity mismatch").into());
        }
        *owned = true;
        let view = Uint8Array::new_with_byte_offset_and_length(&buffer, 0, used_bytes as u32);
        let final_record = view
            .slice((used_bytes - RECORD_BYTES) as u32, used_bytes as u32)
            .to_vec();
        if !shoop_tracing::raw_chunk_ends_group(&final_record) {
            return Err(anyhow!("trace chunk ends inside a record group").into());
        }
        let offset = self.records.len();
        if offset > MAX_REALM_TRACE_BYTES.saturating_sub(used_bytes) {
            self.health.storage_failures = self.health.storage_failures.saturating_add(1);
            return Err(anyhow!("browser trace storage quota exhausted").into());
        }
        self.records
            .try_reserve(used_bytes)
            .map_err(|_| anyhow!("browser trace storage quota exhausted"))?;
        self.records.resize(offset + used_bytes, 0);
        view.copy_to(&mut self.records[offset..]);
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .filter(|value| *value <= MAX_SAFE_INTEGER)
            .ok_or_else(|| {
                anyhow!("trace chunk sequence exceeds JavaScript's safe integer range")
            })?;

        let recycle = Object::new();
        Reflect::set(&recycle, &"kind".into(), &"shoop-trace-recycle".into())?;
        Reflect::set(
            &recycle,
            &"captureId".into(),
            &JsValue::from_f64(self.capture_id as f64),
        )?;
        Reflect::set(
            &recycle,
            &"poolToken".into(),
            &JsValue::from_f64(token as f64),
        )?;
        Reflect::set(&recycle, &"buffer".into(), buffer.as_ref())?;
        let transfer = Array::new();
        transfer.push(buffer.as_ref());
        port.post_message_with_transferable(recycle.as_ref(), &transfer)?;
        *owned = false;
        Ok(())
    }

    fn matches_capture(&self, data: &JsValue) -> Result<bool> {
        Ok(integer(data, "captureId", MAX_SAFE_INTEGER)? == self.capture_id)
    }

    pub fn abort(&mut self) {
        self.abort_with_reason(format!("{} trace collection was aborted", self.label));
    }

    pub fn abort_with_reason(&mut self, reason: impl Into<String>) {
        self.terminal = TerminalState::Aborted(reason.into());
    }

    pub fn stopped(&self) -> bool {
        !matches!(self.terminal, TerminalState::Active)
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn finish(self) -> Result<shoop_tracing::BrowserRealmData> {
        match &self.terminal {
            TerminalState::Active => {
                return Err(anyhow!("{} trace producer has not stopped", self.label).into())
            }
            TerminalState::Aborted(reason) => return Err(anyhow!(reason.clone()).into()),
            TerminalState::Complete => {}
        }
        Ok(shoop_tracing::BrowserRealmData {
            id: self.realm_id,
            label: self.label,
            ticks_per_second: self.ticks_per_second,
            records: self.records,
            metadata: self.metadata,
            calibrations: self.calibrations,
            health: self.health,
        })
    }

    fn add_message_calibration(&mut self, data: &JsValue) -> Result<()> {
        let source_ticks = Reflect::get(data, &"sourceTicks".into())
            .ok()
            .and_then(|value| value.as_f64())
            .map(|value| value.max(0.0).round() as u64);
        let fallback_clock = Reflect::get(data, &"fallbackClock".into())
            .ok()
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let mut uncertainty_ns = 100_000;
        let reference_time_ns = if fallback_clock {
            let sent = Reflect::get(data, &"requestReferenceMs".into())
                .ok()
                .and_then(|value| value.as_f64());
            let received = reference_time_ms().ok();
            match (sent, received) {
                (Some(sent), Some(received)) => {
                    uncertainty_ns =
                        (((received - sent).max(0.0) * 500_000.0).ceil() as u64).max(100_000);
                    Some(((sent + received) * 500_000.0).round() as u64)
                }
                (None, Some(received)) => {
                    uncertainty_ns = 1_000_000_000;
                    Some((received * 1_000_000.0).round() as u64)
                }
                _ => None,
            }
        } else {
            Reflect::get(data, &"referenceMs".into())
                .ok()
                .and_then(|value| value.as_f64())
                .map(|value| (value * 1_000_000.0).round() as u64)
        };
        if let (Some(source_ticks), Some(reference_time_ns)) = (source_ticks, reference_time_ns) {
            if self
                .calibrations
                .last()
                .is_some_and(|previous| previous.source_ticks >= source_ticks)
            {
                return Ok(());
            }
            self.calibrations.push(shoop_tracing::BrowserCalibration {
                realm_id: self.realm_id,
                clock_id: self.clock_id,
                source_ticks,
                reference_time_ns,
                uncertainty_ns,
            });
            return Ok(());
        }
        Err(anyhow!("trace calibration message is incomplete").into())
    }
}
