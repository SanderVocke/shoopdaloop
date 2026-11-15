//! AXI functional bytes, protocol checks, timing submissions and bus accounting.
use super::{Control, Stable};
use crate::{
    model::Model,
    random::Random,
    timing::{Timing, Token, summary},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Address {
    pub address: u64,
    pub id: u64,
    pub length: u64,
    pub size: u64,
    pub burst: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Write {
    pub data: u64,
    pub strobe: u64,
    pub last: u64,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Driven {
    pub aw: u64,
    pub ar: u64,
    pub w: u64,
    pub b: Option<u64>,
    pub r: Option<(u64, u64, u64)>,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Record {
    pub address: u64,
    pub length: u64,
    pub size: u64,
    pub write: bool,
    pub start: u64,
    pub first: u64,
    pub end: u64,
    pub latency: u64,
    pub first_latency: u64,
    pub available_latency: u64,
    pub submission_latency: u64,
    pub beat_cycles: Vec<u64>,
}
pub struct Transfer {
    pub address: Address,
    pub write: bool,
    pub error: bool,
    pub index: usize,
    start: u64,
    blocks: Vec<u64>,
    tokens: BTreeMap<u64, Token>,
    submitted: bool,
    written: bool,
    data: Vec<u64>,
    first: Option<u64>,
    available: Option<u64>,
    submission: Option<u64>,
    beats: Vec<u64>,
}
pub struct Axi {
    pub words: Vec<u32>,
    pub base: u64,
    pub width: usize,
    pub word_only: bool,
    pub timing: Box<dyn Timing>,
    pub active: Option<Transfer>,
    pub records: Vec<Record>,
    pub cycle: u64,
    pub reads: u64,
    pub writes: u64,
    pub native_reads: u64,
    pub native_writes: u64,
    pub address_stalls: u64,
    pub write_stalls: u64,
    pub response_stalls: u64,
    pub submission_stalls: u64,
    random: Random,
    write_beats: Vec<Write>,
    offered_r: Option<(u64, u64, u64)>,
    offered_b: Option<u64>,
    held_aw: Stable<Address>,
    held_ar: Stable<Address>,
    held_w: Stable<Write>,
    resetting: bool,
    pub driven: Driven,
}
impl Axi {
    pub fn new(words: usize, base: u64, width: usize, timing: Box<dyn Timing>, seed: u64) -> Self {
        assert!(matches!(width, 4 | 8) && base % width as u64 == 0);
        Self {
            words: vec![0xa5a5a5a5; words],
            base,
            width,
            word_only: false,
            timing,
            active: None,
            records: Vec::new(),
            cycle: 0,
            reads: 0,
            writes: 0,
            native_reads: 0,
            native_writes: 0,
            address_stalls: 0,
            write_stalls: 0,
            response_stalls: 0,
            submission_stalls: 0,
            random: Random::new(seed),
            write_beats: Vec::new(),
            offered_r: None,
            offered_b: None,
            held_aw: Stable::default(),
            held_ar: Stable::default(),
            held_w: Stable::default(),
            resetting: false,
            driven: Driven::default(),
        }
    }
    pub fn read_bus(&self, address: u64) -> u64 {
        let first = address - address % self.width as u64 - self.base;
        (0..self.width).fold(0, |value, lane| {
            let byte = first as usize + lane;
            let word = self.words.get(byte / 4).copied().unwrap_or(0);
            value | (u64::from((word >> ((byte % 4) * 8)) & 255) << (8 * lane))
        })
    }
    fn write_bus(&mut self, address: u64, beat: Write, size: u64) {
        let lane = address % self.width as u64;
        let mask = ((1u64 << (1 << size)) - 1) << lane;
        assert_eq!(beat.strobe & !mask, 0, "WSTRB outside transfer lanes");
        let first = address - lane - self.base;
        for i in 0..self.width {
            if beat.strobe & (1 << i) != 0 {
                let byte = first as usize + i;
                let shift = (byte % 4) * 8;
                self.words[byte / 4] = (self.words[byte / 4] & !(255 << shift))
                    | ((((beat.data >> (i * 8)) & 255) as u32) << shift);
            }
        }
    }
    pub fn drive(&mut self, m: &mut Model<'_>, c: &Control) {
        let permitted = !c.pause && !self.resetting;
        let mut driven = Driven {
            aw: u64::from(
                permitted
                    && !c.hold_aw
                    && self.active.is_none()
                    && (!c.stalls || self.random.yes(4)),
            ),
            ar: u64::from(
                permitted
                    && !c.hold_ar
                    && self.active.is_none()
                    && self.write_beats.is_empty()
                    && m.get("m_axi_awvalid") == 0
                    && (!c.stalls || self.random.yes(4)),
            ),
            w: u64::from(
                permitted
                    && !c.hold_w
                    && self.active.as_ref().is_none_or(|a| a.write)
                    && self.write_beats.len() < 16
                    && (!c.stalls || self.random.yes(4)),
            ),
            ..Driven::default()
        };
        if let Some(active) = &self.active {
            if permitted && !c.pause_responses {
                if active.write {
                    if active.submitted
                        && active.tokens.values().all(|&t| self.timing.ready(t))
                        && self.offered_b.is_none()
                    {
                        self.offered_b = Some(u64::from(active.error) * 2);
                    }
                } else {
                    let address =
                        active.address.address + ((active.index as u64) << active.address.size);
                    let block = address - address % self.timing.native_bytes();
                    if active
                        .tokens
                        .get(&block)
                        .is_some_and(|&t| self.timing.ready(t))
                        && self.offered_r.is_none()
                    {
                        self.offered_r = Some((
                            active.data[active.index],
                            u64::from(active.error || c.error_index == Some(active.index)) * 2,
                            u64::from(active.index as u64 == active.address.length),
                        ));
                    }
                }
            }
        }
        driven.b = self.offered_b;
        driven.r = self.offered_r;
        if (driven.b.is_some() || driven.r.is_some())
            && let Some(active) = &mut self.active
        {
            active.available.get_or_insert(self.cycle + 1);
        }
        let (data, resp, last) = driven.r.unwrap_or_default();
        m.set_all(&[
            ("m_axi_awready", driven.aw),
            ("m_axi_arready", driven.ar),
            ("m_axi_wready", driven.w),
            ("m_axi_bvalid", u64::from(driven.b.is_some())),
            ("m_axi_bresp", driven.b.unwrap_or(0)),
            ("m_axi_bid", 0),
            ("m_axi_rvalid", u64::from(driven.r.is_some())),
            ("m_axi_rdata", data),
            ("m_axi_rresp", resp),
            ("m_axi_rlast", last),
            ("m_axi_rid", 0),
        ]);
        self.driven = driven;
    }
    fn address(m: &Model<'_>, prefix: &str) -> Address {
        let get = |suffix| m.get(&format!("m_axi_{prefix}{suffix}"));
        Address {
            address: get("addr"),
            id: get("id"),
            length: get("len"),
            size: get("size"),
            burst: get("burst"),
        }
    }
    fn accept(&mut self, a: Address, write: bool, control: &mut Control) {
        let end = a.address + ((a.length + 1) << a.size);
        assert_eq!(a.id, 0);
        assert_eq!(a.burst, 1);
        assert!(a.length < 16 && (1 << a.size) <= self.width);
        assert_eq!(a.address % (1 << a.size), 0);
        assert_eq!(a.address / 4096, (end - 1) / 4096);
        assert!(
            self.base <= a.address
                && a.address < end
                && end <= self.base + self.words.len() as u64 * 4,
            "AXI address outside arena: {a:?}"
        );
        if self.word_only {
            assert_eq!((a.length, a.size), (0, 2));
        }
        let first = a.address - a.address % self.timing.native_bytes();
        let data = (0..=a.length)
            .map(|i| self.read_bus(a.address + (i << a.size)))
            .collect();
        self.active = Some(Transfer {
            address: a,
            write,
            error: std::mem::take(&mut control.fail_next),
            index: 0,
            start: self.cycle,
            blocks: (first..end)
                .step_by(self.timing.native_bytes() as usize)
                .collect(),
            tokens: BTreeMap::new(),
            submitted: false,
            written: false,
            data,
            first: None,
            available: None,
            submission: None,
            beats: Vec::new(),
        });
        self.writes += u64::from(write);
        self.reads += u64::from(!write);
    }
    fn finish(&mut self) {
        let a = self.active.take().unwrap();
        self.records.push(Record {
            address: a.address.address,
            length: a.address.length,
            size: a.address.size,
            write: a.write,
            start: a.start,
            first: a.first.unwrap(),
            end: self.cycle,
            latency: self.cycle - a.start,
            first_latency: a.first.unwrap() - a.start,
            available_latency: a.available.unwrap() - a.start,
            submission_latency: a.submission.unwrap() - a.start,
            beat_cycles: a.beats,
        });
        self.write_beats.clear();
    }
    pub fn sample(&mut self, m: &Model<'_>, control: &mut Control) {
        self.cycle += 1;
        self.timing.advance();
        if m.get("reset") != 0 {
            if !self.resetting {
                self.timing.reset();
            }
            self.resetting = true;
            self.active = None;
            self.write_beats.clear();
            self.offered_r = None;
            self.offered_b = None;
            self.held_aw = Stable::default();
            self.held_ar = Stable::default();
            self.held_w = Stable::default();
            return;
        }
        self.resetting = false;
        let d = self.driven;
        let aw = Self::address(m, "aw");
        let ar = Self::address(m, "ar");
        let awvalid = m.get("m_axi_awvalid");
        let arvalid = m.get("m_axi_arvalid");
        let wvalid = m.get("m_axi_wvalid");
        let w = Write {
            data: m.get("m_axi_wdata"),
            strobe: m.get("m_axi_wstrb"),
            last: m.get("m_axi_wlast"),
        };
        self.held_aw.check("AW", awvalid != 0, d.aw != 0, aw);
        self.held_ar.check("AR", arvalid != 0, d.ar != 0, ar);
        self.held_w.check("W", wvalid != 0, d.w != 0, w);
        self.address_stalls +=
            u64::from(awvalid != 0 && d.aw == 0) + u64::from(arvalid != 0 && d.ar == 0);
        self.write_stalls += u64::from(wvalid != 0 && d.w == 0);
        self.response_stalls += u64::from(d.b.is_some() && m.get("m_axi_bready") == 0)
            + u64::from(d.r.is_some() && m.get("m_axi_rready") == 0);
        if d.b.is_some() && m.get("m_axi_bready") != 0 {
            let a = self.active.as_mut().unwrap();
            a.first.get_or_insert(self.cycle);
            a.beats.push(self.cycle);
            self.offered_b = None;
            self.finish();
        }
        if d.r.is_some() && m.get("m_axi_rready") != 0 {
            let a = self.active.as_mut().unwrap();
            a.first.get_or_insert(self.cycle);
            a.beats.push(self.cycle);
            a.index += 1;
            self.offered_r = None;
            if d.r.unwrap().2 != 0 {
                self.finish();
            }
        }
        if awvalid != 0 && d.aw != 0 {
            assert!(self.active.is_none());
            self.accept(aw, true, control);
        }
        if arvalid != 0 && d.ar != 0 {
            assert!(self.active.is_none());
            self.accept(ar, false, control);
        }
        if wvalid != 0 && d.w != 0 {
            self.write_beats.push(w);
        }
        let Some(active) = &self.active else {
            return;
        };
        if active.write && !active.written {
            let count = active.address.length as usize + 1;
            if self.write_beats.len() < count {
                return;
            }
            assert_eq!(self.write_beats.len(), count);
            let address = active.address;
            let error = active.error;
            for i in 0..count {
                let beat = self.write_beats[i];
                assert_eq!(beat.last != 0, i + 1 == count);
                if !error {
                    self.write_bus(
                        address.address + ((i as u64) << address.size),
                        beat,
                        address.size,
                    );
                }
            }
            self.active.as_mut().unwrap().written = true;
        }
        let active = self.active.as_mut().unwrap();
        for &address in &active.blocks {
            if let std::collections::btree_map::Entry::Vacant(entry) = active.tokens.entry(address)
            {
                let Some(token) = self.timing.submit(address, active.write) else {
                    self.submission_stalls += 1;
                    break;
                };
                entry.insert(token);
                active.submission.get_or_insert(self.cycle);
                self.native_writes += u64::from(active.write);
                self.native_reads += u64::from(!active.write);
            }
        }
        active.submitted = active.tokens.len() == active.blocks.len();
    }
    pub fn report(&mut self, finalize: bool) -> Value {
        let mut groups = serde_json::Map::new();
        for write in [false, true] {
            for burst in [false, true] {
                let rows: Vec<_> = self
                    .records
                    .iter()
                    .filter(|r| r.write == write && (r.length > 0) == burst)
                    .collect();
                let key = format!(
                    "{}_{}",
                    if write { "write" } else { "read" },
                    if burst { "burst" } else { "word" }
                );
                groups.insert(key,json!({"latency_cycles":summary(&rows.iter().map(|r| r.latency).collect::<Vec<_>>()),"first_response_cycles":summary(&rows.iter().map(|r| r.first_latency).collect::<Vec<_>>()),"response_available_cycles":summary(&rows.iter().map(|r| r.available_latency).collect::<Vec<_>>()),"native_submission_cycles":summary(&rows.iter().map(|r| r.submission_latency).collect::<Vec<_>>()),"interbeat_cycles":summary(&rows.iter().flat_map(|r| r.beat_cycles.windows(2).map(|w| w[1]-w[0])).collect::<Vec<_>>())}));
            }
        }
        json!({"cycles":self.cycle,"reads":self.reads,"writes":self.writes,"logical_bytes":self.records.iter().map(|r| (r.length+1)<<r.size).sum::<u64>(),"native_reads":self.native_reads,"native_writes":self.native_writes,"native_requested_bytes":(self.native_reads+self.native_writes)*self.timing.native_bytes(),"address_stall_cycles":self.address_stalls,"write_stall_cycles":self.write_stalls,"response_stall_cycles":self.response_stalls,"native_queue_stall_cycles":self.submission_stalls,"groups":groups,"native":self.timing.report(finalize)})
    }
}
