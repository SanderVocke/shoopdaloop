use anyhow::anyhow;
use js_sys::{Array, ArrayBuffer, Object, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::MessagePort;

const RECORD_BYTES: usize = 48;
const TRACE_PROTOCOL_VERSION: u32 = 2;
const TRACE_POOL_SIZE: usize = 3;

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
    stopped: bool,
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
            capture_id: u64::from(realm_id),
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
            stopped: false,
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
        match kind.as_deref() {
            Some("shoop-trace-metadata") => {
                self.validate_capture(data)?;
                let entries = Array::from(&Reflect::get(data, &"metadata".into())?);
                self.metadata.clear();
                for value in entries.iter() {
                    self.metadata.push(shoop_tracing::BrowserMetadata {
                        id: number(&value, "id")? as u32,
                        namespace: number(&value, "namespace")? as u8,
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
                self.validate_capture(data)?;
                let chunk_count = number(data, "chunkCount")? as u64;
                if chunk_count != self.next_sequence {
                    return Err(anyhow!(
                        "trace stopped with {chunk_count} chunks after receiving {}",
                        self.next_sequence
                    )
                    .into());
                }
                self.health.emitted_records = number(data, "emittedRecords")? as u64;
                self.health.dropped_records = number(data, "droppedRecords")? as u64;
                self.health.raw_dropped_records = number(data, "rawDroppedRecords")? as u64;
                self.health.pool_starvation_records = number(data, "poolStarvationRecords")? as u64;
                self.health.completed_batches = chunk_count;
                self.health.high_water_records = number(data, "highWaterRecords")? as usize;
                self.health.max_in_flight_chunks = number(data, "maxInFlight")? as usize;
                self.health.returned_buffers = number(data, "returnedBuffers")? as u64;
                self.health.rejected_chunks = number(data, "rejectedChunks")? as u64;
                self.add_message_calibration(data)?;
                self.stopped = true;
                tracing::info!(realm = self.realm_id, "frontend.browser_trace.stopped");
                Ok(true)
            }
            Some("shoop-trace-aborted") => {
                self.validate_capture(data)?;
                self.stopped = true;
                Err(anyhow!("{} trace producer aborted", self.label).into())
            }
            _ => Ok(false),
        }
    }

    fn handle_chunk(&mut self, data: &JsValue, port: &MessagePort) -> Result<()> {
        self.validate_capture(data)?;
        let sequence = number(data, "sequence")? as u64;
        if sequence != self.next_sequence {
            return Err(anyhow!(
                "expected trace chunk {}, received {sequence}",
                self.next_sequence
            )
            .into());
        }
        let token = number(data, "poolToken")? as usize;
        let owned = self
            .collector_owned_tokens
            .get_mut(token)
            .ok_or_else(|| anyhow!("unknown trace pool token {token}"))?;
        if *owned {
            return Err(anyhow!("trace pool token {token} is already collector-owned").into());
        }
        let used_bytes = number(data, "usedBytes")? as usize;
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
        let offset = self.records.len();
        self.records
            .try_reserve(used_bytes)
            .map_err(|_| anyhow!("browser trace storage quota exhausted"))?;
        self.records.resize(offset + used_bytes, 0);
        view.copy_to(&mut self.records[offset..]);
        self.next_sequence = self.next_sequence.saturating_add(1);

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

    fn validate_capture(&self, data: &JsValue) -> Result<()> {
        let capture_id = number(data, "captureId")? as u64;
        if capture_id != self.capture_id {
            return Err(anyhow!("stale trace capture {capture_id}").into());
        }
        Ok(())
    }

    pub fn abort(&mut self) {
        self.stopped = true;
    }

    pub fn stopped(&self) -> bool {
        self.stopped
    }

    pub fn finish(self) -> Result<shoop_tracing::BrowserRealmData> {
        if !self.stopped {
            return Err(anyhow!("{} trace producer has not stopped", self.label).into());
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
