//! The existing DRAMsim3 C ABI owns its controller; Rust owns the handle and library.
use super::{Timing, Token, histogram_summary, summary};
use crate::Result;
use libloading::Library;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{CStr, CString, c_char, c_void},
    path::{Path, PathBuf},
    ptr::NonNull,
};

#[repr(C)]
#[derive(Default, Clone, Copy, Debug)]
pub struct Completion {
    pub id: u64,
    pub address: u64,
    pub submitted: u64,
    pub completed: u64,
    pub write: u32,
}
#[repr(C)]
#[derive(Default, Clone, Copy, Debug, serde::Serialize)]
pub struct Bus {
    pub ticks: u64,
    pub read_cycles: u64,
    pub write_cycles: u64,
    pub read_commands: u64,
    pub write_commands: u64,
    pub command_cycles: u64,
}
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct Snapshot {
    pub generation: u64,
    pub counters: Bus,
}
struct Api {
    create: unsafe extern "C" fn(*const c_char, *const c_char) -> *mut c_void,
    destroy: unsafe extern "C" fn(*mut c_void),
    error: unsafe extern "C" fn() -> *const c_char,
    native_bytes: unsafe extern "C" fn(*mut c_void) -> u32,
    tck_fs: unsafe extern "C" fn(*mut c_void) -> u64,
    ticks: unsafe extern "C" fn(*mut c_void) -> u64,
    submit: unsafe extern "C" fn(*mut c_void, u64, u32) -> u64,
    advance: unsafe extern "C" fn(*mut c_void, u64),
    poll: unsafe extern "C" fn(*mut c_void, *mut Completion) -> u32,
    idle: unsafe extern "C" fn(*mut c_void) -> u32,
    drain: unsafe extern "C" fn(*mut c_void, u64) -> u32,
    print_stats: unsafe extern "C" fn(*mut c_void),
    bus_snapshot: unsafe extern "C" fn(*mut c_void, *mut Bus),
}
pub struct Dram {
    api: Api,
    handle: NonNull<c_void>,
    config: CString,
    output_c: CString,
    pub output: PathBuf,
    pub capacity: u64,
    pub channels: u64,
    pub bus_width: u64,
    pub fabric_hz: u64,
    pub tck_fs: u64,
    pub native_bytes: u64,
    pub generation: u64,
    pub events: Vec<Completion>,
    done: BTreeSet<Token>,
    final_statistics: Option<Value>,
    provenance: Value,
    _library: Library,
}
impl Dram {
    pub fn new(library: &Path, config: &Path, output: &Path, fabric_hz: u64) -> Result<Self> {
        if !(1_000_000..=1_000_000_000).contains(&fabric_hz) {
            return Err("fabric frequency must be 1 MHz to 1 GHz".into());
        }
        let settings = ini(config)?;
        let provenance = json!({"library":library,"library_sha256":crate::report::sha256(library)?,"config":config,"config_sha256":crate::report::sha256(config)?});
        if settings["dram_structure.protocol"] != "DDR3"
            || settings["system.unified_queue"] != "true"
            || settings["timing.tREFI"].parse::<u64>()? == 0
        {
            return Err("use a DDR3 unified-queue profile with positive refresh interval".into());
        }
        let number = |key: &str| -> Result<u64> { Ok(settings[key].parse()?) };
        let capacity = number("system.channel_size")? * 1024 * 1024 * number("system.channels")?;
        std::fs::create_dir_all(output)?;
        let config = CString::new(config.as_os_str().as_encoded_bytes())?;
        let output_c = CString::new(format!("{}/", output.display()))?;
        // SAFETY: the ABI version is checked before calling its fixed C signatures.
        let (library, api, handle, native_bytes, tck_fs) = unsafe {
            let library = Library::new(library)?;
            let abi: libloading::Symbol<unsafe extern "C" fn() -> u32> =
                library.get(b"shoop_dram_abi_version")?;
            if abi() != 2 {
                return Err("DRAMsim3 ABI 2 required; rebuild the native bridge".into());
            }
            let api = Api {
                create: *library.get(b"shoop_dram_create")?,
                destroy: *library.get(b"shoop_dram_destroy")?,
                error: *library.get(b"shoop_dram_error")?,
                native_bytes: *library.get(b"shoop_dram_native_bytes")?,
                tck_fs: *library.get(b"shoop_dram_tck_fs")?,
                ticks: *library.get(b"shoop_dram_ticks")?,
                submit: *library.get(b"shoop_dram_submit")?,
                advance: *library.get(b"shoop_dram_advance")?,
                poll: *library.get(b"shoop_dram_poll")?,
                idle: *library.get(b"shoop_dram_idle")?,
                drain: *library.get(b"shoop_dram_drain")?,
                print_stats: *library.get(b"shoop_dram_print_stats")?,
                bus_snapshot: *library.get(b"shoop_dram_bus_snapshot")?,
            };
            let handle = NonNull::new((api.create)(config.as_ptr(), output_c.as_ptr()))
                .ok_or_else(|| CStr::from_ptr((api.error)()).to_string_lossy().into_owned())?;
            let native_bytes = u64::from((api.native_bytes)(handle.as_ptr()));
            let tck_fs = (api.tck_fs)(handle.as_ptr());
            (library, api, handle, native_bytes, tck_fs)
        };
        Ok(Self {
            api,
            handle,
            config,
            output_c,
            output: output.into(),
            capacity,
            channels: number("system.channels")?,
            bus_width: number("system.bus_width")?,
            fabric_hz,
            tck_fs,
            native_bytes,
            generation: 0,
            events: Vec::new(),
            done: BTreeSet::new(),
            final_statistics: None,
            provenance,
            _library: library,
        })
    }
    pub fn default(output: &Path, fabric_hz: u64) -> Result<Self> {
        let root = crate::root();
        Self::new(
            &root.join("work/dramsim3/build/libshoop_dram.so"),
            &root.join("fpga/axi_memory/configs/zynq7010-ddr3-1066-x16.ini"),
            output,
            fabric_hz,
        )
    }
    fn poll(&mut self) {
        let mut event = Completion::default();
        // SAFETY: the model handle and output buffer live throughout the call.
        while unsafe { (self.api.poll)(self.handle.as_ptr(), &mut event) } != 0 {
            self.done.insert(Token {
                generation: self.generation,
                id: event.id,
            });
            self.events.push(event);
        }
    }
    pub fn ticks(&self) -> u64 {
        unsafe { (self.api.ticks)(self.handle.as_ptr()) }
    }
    pub fn idle(&self) -> bool {
        unsafe { (self.api.idle)(self.handle.as_ptr()) != 0 }
    }
    pub fn drain(&mut self, limit: u64) {
        assert!(
            unsafe { (self.api.drain)(self.handle.as_ptr(), limit) } != 0,
            "DRAM failed to drain"
        );
        self.poll();
    }
    pub fn snapshot(&self) -> Snapshot {
        let mut counters = Bus::default();
        unsafe {
            (self.api.bus_snapshot)(self.handle.as_ptr(), &mut counters);
        }
        Snapshot {
            generation: self.generation,
            counters,
        }
    }
    pub fn utilization(&self, start: Snapshot, end: Snapshot) -> Value {
        assert_eq!(
            start.generation, end.generation,
            "utilization crossed a model reset"
        );
        let a = serde_json::to_value(start.counters).unwrap();
        let b = serde_json::to_value(end.counters).unwrap();
        let mut delta = serde_json::Map::new();
        for (key, value) in b.as_object().unwrap() {
            delta.insert(
                key.clone(),
                json!(
                    value
                        .as_u64()
                        .unwrap()
                        .checked_sub(a[key].as_u64().unwrap())
                        .expect("backward utilization window")
                ),
            );
        }
        let ticks = delta.remove("ticks").unwrap().as_u64().unwrap();
        assert!(ticks > 0);
        let read = delta["read_cycles"].as_u64().unwrap();
        let write = delta["write_cycles"].as_u64().unwrap();
        let busy = read + write;
        let capacity = ticks * self.channels;
        let bytes = busy * (self.bus_width / 8) * 2;
        let seconds = ticks as f64 * self.tck_fs as f64 / 1e15;
        let mut result = json!({"dram_ticks":ticks,"channels":self.channels,"elapsed_seconds":seconds,"data_busy_cycles":busy,"data_bus_utilization":busy as f64/capacity as f64,"read_data_bus_utilization":read as f64/capacity as f64,"write_data_bus_utilization":write as f64/capacity as f64,"command_bus_utilization":delta["command_cycles"].as_u64().unwrap() as f64/capacity as f64,"transferred_bytes":bytes,"transfer_bytes_per_second":bytes as f64/seconds,"peak_bytes_per_second":(self.bus_width/8*2*self.channels) as f64*1e15/self.tck_fs as f64});
        result.as_object_mut().unwrap().extend(delta);
        result
    }
}
impl Timing for Dram {
    fn bus_snapshot(&self) -> Option<Snapshot> {
        Some(self.snapshot())
    }
    fn bus_utilization(&self, start: Snapshot, end: Snapshot) -> Option<Value> {
        Some(self.utilization(start, end))
    }
    fn native_bytes(&self) -> u64 {
        self.native_bytes
    }
    fn advance(&mut self) {
        assert!(
            self.final_statistics.is_none(),
            "cannot advance finalized DRAM statistics"
        );
        unsafe {
            (self.api.advance)(self.handle.as_ptr(), self.fabric_hz);
        }
        self.poll();
    }
    fn submit(&mut self, address: u64, write: bool) -> Option<Token> {
        assert!(
            self.final_statistics.is_none(),
            "cannot submit after finalization"
        );
        assert!(
            address % self.native_bytes == 0 && address < self.capacity,
            "native DRAM address out of range or unaligned"
        );
        let id = unsafe { (self.api.submit)(self.handle.as_ptr(), address, u32::from(write)) };
        (id != 0).then_some(Token {
            generation: self.generation,
            id,
        })
    }
    fn ready(&self, token: Token) -> bool {
        self.done.contains(&token)
    }
    fn reset(&mut self) {
        unsafe {
            (self.api.destroy)(self.handle.as_ptr());
        }
        self.generation += 1;
        self.done.clear();
        self.final_statistics = None;
        self.handle = NonNull::new(unsafe {
            (self.api.create)(self.config.as_ptr(), self.output_c.as_ptr())
        })
        .expect("DRAM recreation failed");
    }
    fn report(&mut self, finalize: bool) -> Value {
        if finalize && self.final_statistics.is_none() {
            let before = self.ticks();
            self.drain(1_000_000);
            unsafe {
                (self.api.print_stats)(self.handle.as_ptr());
            }
            let statistics: Value =
                serde_json::from_slice(&std::fs::read(self.output.join("dramsim3.json")).unwrap())
                    .unwrap();
            let mut reads = 0;
            let mut writes = 0;
            let mut refresh = 0;
            let mut rh = BTreeMap::new();
            let mut wh = BTreeMap::new();
            for channel in statistics.as_object().unwrap().values() {
                reads += channel["num_read_cmds"].as_u64().unwrap();
                writes += channel["num_write_cmds"].as_u64().unwrap();
                refresh += channel["num_ref_cmds"].as_u64().unwrap();
                for (key, histogram) in [("read_latency", &mut rh), ("write_latency", &mut wh)] {
                    if let Some(values) = channel[key].as_object() {
                        for (n, count) in values {
                            *histogram.entry(n.parse().unwrap()).or_insert(0) +=
                                count.as_u64().unwrap();
                        }
                    }
                }
            }
            let mut final_stats = json!({"dram_read_commands":reads,"dram_write_commands":writes,"dram_command_bytes":(reads+writes)*self.native_bytes,"refresh_commands":refresh,"dram_read_service_dram_cycles":histogram_summary(&rh),"dram_write_service_dram_cycles":histogram_summary(&wh),"final_drain_dram_ticks":self.ticks()-before});
            if self.ticks() > 0 {
                final_stats["whole_model_bus"] = self.utilization(
                    Snapshot {
                        generation: self.generation,
                        counters: Bus::default(),
                    },
                    self.snapshot(),
                );
            }
            self.final_statistics = Some(final_stats);
        }
        let read: Vec<_> = self
            .events
            .iter()
            .filter(|e| e.write == 0)
            .map(|e| e.completed - e.submitted)
            .collect();
        let write: Vec<_> = self
            .events
            .iter()
            .filter(|e| e.write != 0)
            .map(|e| e.completed - e.submitted)
            .collect();
        let mut result = json!({"configuration":self.provenance,"tck_fs":self.tck_fs,"native_bytes":self.native_bytes,"fabric_hz":self.fabric_hz,"abi_version":2,"ticks":self.ticks(),"idle":self.idle(),"physical_counts_generation":self.generation,"physical_counts_finalized":self.final_statistics.is_some(),"read_service_dram_cycles":summary(&read),"write_admission_dram_cycles":summary(&write)});
        if let Some(final_stats) = &self.final_statistics {
            result
                .as_object_mut()
                .unwrap()
                .extend(final_stats.as_object().unwrap().clone());
        }
        result
    }
}
impl Drop for Dram {
    fn drop(&mut self) {
        unsafe {
            (self.api.destroy)(self.handle.as_ptr());
        }
    }
}
fn ini(path: &Path) -> Result<BTreeMap<String, String>> {
    let mut values = BTreeMap::new();
    let mut section = "";
    let text = std::fs::read_to_string(path)?;
    for line in text
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with(';') && !s.starts_with('#'))
    {
        if line.starts_with('[') && line.ends_with(']') {
            section = &line[1..line.len() - 1];
        } else if let Some((key, value)) = line.split_once('=') {
            values.insert(format!("{section}.{}", key.trim()), value.trim().into());
        }
    }
    Ok(values)
}
