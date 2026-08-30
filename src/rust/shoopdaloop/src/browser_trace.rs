use anyhow::anyhow;
use js_sys::{Array, Atomics, Int32Array, Object, Reflect, SharedArrayBuffer, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};

const HEADER_WORDS: u32 = 16;
const HEADER_BYTES: u32 = HEADER_WORDS * 4;
const RECORD_BYTES: u32 = 48;
const MAGIC: i32 = 0x5045_4631;

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

pub struct RealmTraceState {
    realm_id: u32,
    clock_id: u32,
    label: String,
    ticks_per_second: u64,
    capacity: u32,
    sab: SharedArrayBuffer,
    header: Int32Array,
    metadata: Vec<shoop_tracing::BrowserMetadata>,
    calibrations: Vec<shoop_tracing::BrowserCalibration>,
    records: Vec<u8>,
    retention_dropped_records: u64,
    stopped: bool,
}

impl RealmTraceState {
    pub fn new(
        realm_id: u32,
        clock_id: u32,
        label: impl Into<String>,
        ticks_per_second: u64,
        quantum_frames: u32,
        capacity: u32,
    ) -> Result<Self> {
        let constructor = Reflect::get(&js_sys::global(), &JsValue::from_str("SharedArrayBuffer"))?
            .dyn_into::<js_sys::Function>()
            .map_err(|_| anyhow!("SharedArrayBuffer is unavailable; serve with COOP/COEP"))?;
        let arguments = Array::new();
        arguments.push(&JsValue::from_f64(
            (HEADER_BYTES + capacity * RECORD_BYTES) as f64,
        ));
        let sab = Reflect::construct(&constructor, &arguments)?
            .dyn_into::<SharedArrayBuffer>()
            .map_err(|_| anyhow!("SharedArrayBuffer construction returned the wrong type"))?;
        let header = Int32Array::new_with_byte_offset_and_length(&sab, 0, HEADER_WORDS);
        header.set_index(0, MAGIC);
        header.set_index(1, capacity as i32);
        header.set_index(9, ticks_per_second.min(i32::MAX as u64) as i32);
        header.set_index(10, quantum_frames as i32);
        Ok(Self {
            realm_id,
            clock_id,
            label: label.into(),
            ticks_per_second,
            capacity,
            sab,
            header,
            metadata: Vec::new(),
            calibrations: Vec::new(),
            records: Vec::new(),
            retention_dropped_records: 0,
            stopped: false,
        })
    }

    pub fn start_message(&self, engine_detail: bool) -> Result<JsValue> {
        let message = Object::new();
        Reflect::set(&message, &"kind".into(), &"shoop-trace-start".into())?;
        Reflect::set(&message, &"sab".into(), self.sab.as_ref())?;
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
            &JsValue::from_f64(self.capacity as f64),
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

    pub fn stop_message() -> Result<JsValue> {
        let message = Object::new();
        Reflect::set(&message, &"kind".into(), &"shoop-trace-stop".into())?;
        Reflect::set(
            &message,
            &"referenceMs".into(),
            &JsValue::from_f64(reference_time_ms()?),
        )?;
        Ok(message.into())
    }

    pub fn handle_message(&mut self, data: &JsValue) -> Result<bool> {
        let kind = Reflect::get(data, &"kind".into())
            .ok()
            .and_then(|value| value.as_string());
        match kind.as_deref() {
            Some("shoop-trace-metadata") => {
                let entries = Array::from(&Reflect::get(data, &"metadata".into())?);
                self.metadata.clear();
                for value in entries.iter() {
                    self.metadata.push(shoop_tracing::BrowserMetadata {
                        id: Reflect::get(&value, &"id".into())?.as_f64().unwrap_or(0.0) as u32,
                        namespace: Reflect::get(&value, &"namespace".into())?
                            .as_f64()
                            .unwrap_or(0.0) as u8,
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
            Some("shoop-trace-stopped") => {
                self.poll()?;
                self.stopped = true;
                self.add_message_calibration(data)?;
                tracing::info!(realm = self.realm_id, "frontend.browser_trace.stopped");
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub fn poll(&mut self) -> Result<()> {
        let write = Atomics::load(&self.header, 2)? as u32;
        let read = Atomics::load(&self.header, 3)? as u32;
        let available = write.wrapping_sub(read).min(self.capacity);
        if available == 0 {
            return Ok(());
        }
        let data = Uint8Array::new_with_byte_offset(&self.sab, HEADER_BYTES);
        let mut drained = vec![0_u8; available as usize * RECORD_BYTES as usize];
        for offset in 0..available {
            let slot = read.wrapping_add(offset) % self.capacity;
            let source = slot * RECORD_BYTES;
            let destination = offset as usize * RECORD_BYTES as usize;
            data.slice(source, source + RECORD_BYTES)
                .copy_to(&mut drained[destination..destination + RECORD_BYTES as usize]);
        }
        Atomics::store(&self.header, 3, write as i32)?;
        shoop_tracing::append_bounded_browser_records(
            &mut self.records,
            &mut self.retention_dropped_records,
            &drained,
        )
        .map_err(BrowserTraceError)?;
        Ok(())
    }

    pub fn abort(&mut self) -> Result<()> {
        self.poll()?;
        self.stopped = true;
        Ok(())
    }

    pub fn stopped(&self) -> bool {
        self.stopped
    }

    pub fn finish(mut self) -> Result<shoop_tracing::BrowserRealmData> {
        if !self.stopped {
            return Err(anyhow!("{} trace producer has not stopped", self.label).into());
        }
        self.poll()?;
        let emitted_records = self.records.len() / RECORD_BYTES as usize;
        Ok(shoop_tracing::BrowserRealmData {
            id: self.realm_id,
            label: self.label,
            ticks_per_second: self.ticks_per_second,
            records: self.records,
            metadata: self.metadata,
            calibrations: self.calibrations,
            health: shoop_tracing::BrowserHealth {
                emitted_records: emitted_records as u64,
                dropped_records: u64::from(Atomics::load(&self.header, 4)? as u32)
                    .saturating_add(self.retention_dropped_records),
                completed_batches: u64::from(Atomics::load(&self.header, 5)? as u32),
                high_water_records: Atomics::load(&self.header, 8)? as u32 as usize,
                repaired_span_boundaries: 0,
            },
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
        self.add_calibration()
    }

    fn add_calibration(&mut self) -> Result<()> {
        let low = Atomics::load(&self.header, 11)? as u32;
        let high = Atomics::load(&self.header, 12)? as u32;
        let source_ticks = u64::from(low) | (u64::from(high) << 32);
        if self
            .calibrations
            .last()
            .is_some_and(|previous| previous.source_ticks >= source_ticks)
        {
            return Ok(());
        }
        let performance = web_sys::window()
            .ok_or_else(|| anyhow!("browser window is unavailable"))?
            .performance()
            .ok_or_else(|| anyhow!("browser performance clock is unavailable"))?;
        let before = performance.now();
        let after = performance.now();
        let reference_ms = performance.time_origin() + (before + after) * 0.5;
        self.calibrations.push(shoop_tracing::BrowserCalibration {
            realm_id: self.realm_id,
            clock_id: self.clock_id,
            source_ticks,
            reference_time_ns: (reference_ms * 1_000_000.0).round() as u64,
            uncertainty_ns: (((after - before) * 500_000.0).round() as u64).max(1),
        });
        Ok(())
    }
}
