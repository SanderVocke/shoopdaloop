use super::{
    Geometry, Memory, Mode, Sim, design, finish_request, first_fault, publish_hold, read,
    read_hold, release_read, release_write, start_request, upload, write, write_hold,
};
use rt_fpga_rtl::host::semantic_registers::*;
use rt_fpga_rtl::{clock::Edges, model::Model, sim::Agent};
use rt_fpga_testbench::model::{Design, Probe};
use serde_json::json;
use shoop_wasm_test_support::shoop_test;
use std::{cell::RefCell, rc::Rc};

fn paced_design(i2s: bool) -> Design {
    let root = rt_fpga_testbench::root();
    let mut d = design().parameters(&[("OUTPUT_PORTS", 2), ("I2S_AUDIO", u64::from(i2s))]);
    d.top = "fifo_audio_host_axi".into();
    d.probes.clear();
    for path in [
        "generated.audio.io.capture_write",
        "generated.audio.generated_sample",
    ] {
        if !i2s {
            d.probes.push(Probe::observe(path));
        }
    }
    for file in [
        "fifo_audio_host_axi.sv",
        "host_generated_audio.v",
        "host_i2s_audio.v",
    ] {
        d.sources.push(root.join("fpga/host_bridge/rtl").join(file));
    }
    for file in ["audio_io.sv", "audio_lane_ram.sv", "i2s_duplex.sv"] {
        d.sources.push(root.join("fpga/audio_io/rtl").join(file));
    }
    d
}
fn initialize(s: &mut Sim<'_>) {
    s.model.set_all(&[("reset", 1), ("epoch", 1)]);
    s.idle(40);
    s.model.set("reset", 0);
    s.until(10_000, "initial snapshot with audio disabled", |m| {
        m.get("busy") == 0
    });
    assert_eq!(s.model.get("audio_enable"), 0);
    assert_eq!(s.model.get("sync_frame"), 0);
    assert_eq!(read(s, REG_SNAPSHOT_READY), 1);
}
fn install(s: &mut Sim<'_>) {
    for (address, instruction) in [
        json!({"kind":"process_input","port":0,"destination":0}),
        json!({"kind":"process_output","source":0,"port":0}),
        json!({"kind":"process_output","source":0,"port":1}),
    ]
    .iter()
    .enumerate()
    {
        upload(s, address as u64, instruction);
    }
    let mut select = [0; 12];
    select[0] = 5;
    select[6] = 1;
    select[8] = 3 << 16;
    select[10] = 1;
    publish_hold(s, 0, 1, 0, &[select]);
    release_write(s);
    s.idle(80_000);
    assert_eq!(s.model.get("active_revision"), 0);
    assert_eq!(
        s.model.get("sync_frame"),
        0,
        "publication is not audio pacing"
    );
}
fn enable_hold(s: &mut Sim<'_>, tag: u32) {
    write(s, REG_REQUEST_EPOCH, s.model.get("runtime_epoch") as u32);
    write(s, REG_REQUEST_OP, SEMANTIC_SERVICE_ENABLE_AUDIO);
    for index in 0..12 {
        write(s, REG_REQUEST_DATA + 4 * index, u32::from(index == 0));
    }
    write_hold(s, REG_REQUEST_PUSH, tag);
}
#[derive(Default)]
struct AudioOracle {
    inputs: Vec<u32>,
    checked: usize,
    captured: bool,
    stopped: bool,
}
impl Agent for AudioOracle {
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if e.rises(0) && !self.stopped {
            self.captured = m.get("generated.audio.io.capture_write") != 0;
            if self.captured {
                self.inputs
                    .push(m.get("generated.audio.generated_sample") as u32);
            }
        }
    }
    fn observe(&mut self, m: &Model<'_>, e: Edges) {
        if e.rises(0) && self.captured && !self.stopped {
            assert_eq!(m.get("tx_valid"), 1);
            let expected = if self.inputs.len() > 64 {
                self.checked += 1;
                u64::from(self.inputs[self.inputs.len() - 65])
            } else {
                0
            };
            assert_eq!(m.get("tx_samples"), expected | (expected << 32));
        }
    }
}

#[shoop_test(no_wasm = "Native audio-owned pacing, real SP/JP and AXI with ARM responses held")]
fn public_enable_uses_real_audio_ticks_and_never_waits_for_publication_or_reply_readers() {
    let root = rt_fpga_testbench::root();
    let compiled = paced_design(false)
        .build(&root.join("work/marlin-suite/semantic-paced"))
        .unwrap();
    let mut s = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
    let geometry = Geometry::new([8, 64, 32], 2);
    let memory = Memory::attach(
        &mut s,
        Mode::Fixed,
        geometry.words() + 128,
        0x0100_0000,
        &root.join("work/marlin-suite/semantic-paced/memory"),
        149,
    )
    .unwrap();
    memory.borrow_mut().control.stalls = true;
    let oracle = Rc::new(RefCell::new(AudioOracle::default()));
    s.agent(oracle.clone());
    initialize(&mut s);
    install(&mut s);
    enable_hold(&mut s, 71);
    s.until(
        350_000,
        "64 exact stereo frames with enable B and all replies unread",
        |m| {
            assert_eq!(m.get("faulted"), 0);
            oracle.borrow().checked >= 64
        },
    );
    assert_eq!(s.model.get("s_axi_bvalid"), 1);
    assert_eq!(s.model.get("audio_enable"), 1);
    assert_eq!(s.model.get("active_revision"), 1);
    assert_eq!(read_hold(&mut s, REG_RESPONSE_TAG), 71);
    release_write(&mut s);
    write(&mut s, REG_RESPONSE_POP, 71);
    let mut enabled = [0; 12];
    enabled[0] = 1;
    super::configuration::request_writes(&mut s, SEMANTIC_SERVICE_ENABLE_AUDIO, &enabled, 72);
    s.idle(140_000);
    assert_eq!(s.model.get("s_axi_rvalid"), 1);
    assert_eq!(s.model.get("s_axi_rdata"), 71);
    assert!(oracle.borrow().checked > 120);
    release_read(&mut s, 71);
    assert_eq!(finish_request(&mut s, 72), [0; 12]);
    assert_eq!(read(&mut s, REG_AUDIO_XRUNS), 0);
    assert!(read(&mut s, REG_SNAPSHOT_SKIPPED_LOW) >= 4);
    assert_eq!(s.model.get("result_ready_mask"), 1);
    write(&mut s, REG_REQUEST_EPOCH, 1);
    write(&mut s, REG_REQUEST_OP, SEMANTIC_SERVICE_ENABLE_AUDIO);
    for index in 0..12 {
        write(&mut s, REG_REQUEST_DATA + 4 * index, 0);
    }
    s.until(
        80_000,
        "already accepted processing before disable publication",
        |m| m.get("busy") != 0,
    );
    oracle.borrow_mut().stopped = true;
    write_hold(&mut s, REG_REQUEST_PUSH, 73);
    s.until(64, "local disable, not a cancellation", |m| {
        m.get("audio_enable") == 0
    });
    assert_ne!(
        s.model.get("busy"),
        0,
        "disable must not manufacture retirement"
    );
    release_write(&mut s);
    assert_eq!(finish_request(&mut s, 73), [0; 12]);
    s.until(
        20_000,
        "accepted quantum still finishes after disable",
        |m| m.get("busy") == 0,
    );
    let frame = s.model.get("sync_frame");
    assert_eq!(s.model.get("runtime_epoch"), 1);
    s.idle(150_000);
    assert_eq!(s.model.get("sync_frame"), frame);
    start_request(&mut s, SEMANTIC_SERVICE_ENABLE_AUDIO, &enabled, 74);
    super::configuration::response_ready(&mut s);
    assert_eq!(
        read(&mut s, REG_RESPONSE_CODE),
        2,
        "stopped is not a fresh incarnation"
    );
    assert_eq!(s.model.get("audio_enable"), 0);
    assert_eq!(s.model.get("faulted"), 0);
}

#[shoop_test(no_wasm = "Native physical watchdog qualification and real AXI retirement/reset")]
fn audio_watchdog_halts_without_auto_recovery_and_old_service_halves_cannot_reenable_after_reset() {
    let root = rt_fpga_testbench::root();
    let compiled = paced_design(false)
        .build(&root.join("work/marlin-suite/semantic-paced"))
        .unwrap();
    for aw_first in [false, true] {
        let mut s = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
        let geometry = Geometry::new([8, 64, 32], 2);
        let memory = Memory::attach(
            &mut s,
            Mode::Fixed,
            geometry.words() + 128,
            0x0100_0000,
            &root.join("work/marlin-suite/semantic-paced/memory"),
            151 + u64::from(aw_first),
        )
        .unwrap();
        memory.borrow_mut().control.stalls = true;
        initialize(&mut s);
        install(&mut s);
        memory.borrow_mut().control.hold_ar = true;
        let mut enabled = [0; 12];
        enabled[0] = 1;
        start_request(&mut s, SEMANTIC_SERVICE_ENABLE_AUDIO, &enabled, 81);
        finish_request(&mut s, 81);
        s.until(
            100_000,
            "real held DDR AR under audio-owned processing",
            |m| m.get("m_axi_arvalid") != 0,
        );
        let address = s.model.get("m_axi_araddr");
        s.until(
            70_000,
            "existing audio I/O watchdog, not an ARM-injected sideband",
            |m| m.get("faulted") != 0,
        );
        s.idle(5);
        assert_eq!(first_fault(&mut s)[1], 32);
        assert_eq!(s.model.get("audio_enable"), 0);
        assert_eq!(s.model.get("runtime_epoch"), 1);
        assert_eq!(s.model.get("runtime_reset"), 0);
        assert_eq!(s.model.get("m_axi_arvalid"), 1);
        assert_eq!(s.model.get("m_axi_araddr"), address);
        assert_eq!(read_hold(&mut s, REG_FAULT_CAUSE), 32);
        write_hold(&mut s, REG_RECOVERY_REQUEST, 1);
        release_write(&mut s);
        s.model.set_all(&[
            ("s_axi_awaddr", u64::from(REG_REQUEST_PUSH)),
            ("s_axi_wdata", 82),
            ("s_axi_wstrb", 15),
        ]);
        let (first, second) = if aw_first {
            (
                ("s_axi_awvalid", "s_axi_awready"),
                ("s_axi_wvalid", "s_axi_wready"),
            )
        } else {
            (
                ("s_axi_wvalid", "s_axi_wready"),
                ("s_axi_awvalid", "s_axi_awready"),
            )
        };
        super::channel(&mut s, first.0, first.1);
        s.idle(128);
        assert_eq!(s.model.get("runtime_epoch"), 1);
        assert_eq!(s.model.get("runtime_reset"), 0);
        memory.borrow_mut().control.hold_ar = false;
        s.until(
            10_000,
            "real response drain then fresh runtime reset",
            |m| {
                m.get("runtime_epoch") == 2
                    && m.get("runtime_reset") == 0
                    && m.get("recovery_state") == 3
            },
        );
        s.until(
            10_000,
            "fresh initial snapshot independent of copied R",
            |m| m.get("busy") == 0,
        );
        assert_eq!(s.model.get("sync_frame"), 0);
        assert_eq!(s.model.get("audio_enable"), 0);
        assert_eq!(s.model.get("s_axi_rvalid"), 1);
        assert_eq!(s.model.get("s_axi_rdata"), 32);
        super::channel(&mut s, second.0, second.1);
        s.until(32, "old service half gets its own refusal B", |m| {
            m.get("s_axi_bvalid") != 0
        });
        release_write(&mut s);
        release_read(&mut s, 32);
        assert_eq!(read(&mut s, REG_SUBMISSION_RESULT), RECEIPT_EPOCH);
        assert_eq!(s.model.get("audio_enable"), 0);
        start_request(&mut s, SEMANTIC_SERVICE_ENABLE_AUDIO, &enabled, 83);
        finish_request(&mut s, 83);
        s.until(
            100_000,
            "fresh audio-owned empty-runtime F0 after reset",
            |m| m.get("sync_frame") == 32,
        );
        assert_eq!(s.model.get("faulted"), 0);
        assert_eq!(
            first_fault(&mut s)[1],
            32,
            "fresh startup does not relabel retained cause"
        );
    }
}

#[derive(Default)]
struct SerialOracle {
    bclk: u64,
    channel: usize,
    slot: usize,
    decoded: u32,
    samples: Vec<u32>,
}
impl Agent for SerialOracle {
    fn observe(&mut self, m: &Model<'_>, e: Edges) {
        if !e.rises(0) {
            return;
        }
        let bclk = m.get("i2s_bclk");
        if bclk == self.bclk {
            return;
        }
        if bclk == 0 {
            let channel = m.get("i2s_lrclk") as usize;
            if channel != self.channel {
                self.channel = channel;
                self.slot = 0;
                self.decoded = 0;
            } else {
                self.slot += 1;
            }
        } else if (1..=24).contains(&self.slot) {
            self.decoded = (self.decoded << 1) | m.get("i2s_dac_data") as u32;
            if self.slot == 24 {
                self.samples.push(self.decoded);
            }
        }
        self.bclk = bclk;
    }
    fn drive(&mut self, m: &mut Model<'_>, _e: Edges) {
        let word = if self.channel == 0 {
            0x12_3456_u32
        } else {
            0xec_ba98
        };
        let bit = if (1..=24).contains(&self.slot) {
            (word >> (24 - self.slot)) & 1
        } else {
            0
        };
        m.set("i2s_adc_data", u64::from(bit));
    }
}

#[shoop_test(no_wasm = "Native asynchronous I2S ADC/SP/JP/DAC path, no CPU quantum requests")]
fn public_enable_drives_real_i2s_audio_through_the_same_semantic_pipeline() {
    let root = rt_fpga_testbench::root();
    let compiled = paced_design(true)
        .build(&root.join("work/marlin-suite/semantic-paced"))
        .unwrap();
    let mut s = Sim::new(
        compiled.session(None).unwrap(),
        &[("clk", 10_000, 0), ("audio_ref", 81_380, 230)],
    );
    let geometry = Geometry::new([8, 64, 32], 2);
    let memory = Memory::attach(
        &mut s,
        Mode::Fixed,
        geometry.words() + 128,
        0x0100_0000,
        &root.join("work/marlin-suite/semantic-paced/memory"),
        157,
    )
    .unwrap();
    memory.borrow_mut().control.stalls = true;
    let serial = Rc::new(RefCell::new(SerialOracle::default()));
    s.agent(serial.clone());
    initialize(&mut s);
    install(&mut s);
    enable_hold(&mut s, 91);
    s.idle(260_000);
    assert_eq!(s.model.get("s_axi_bvalid"), 1);
    assert_eq!(s.model.get("faulted"), 0);
    assert_eq!(s.model.get("active_revision"), 1);
    assert!(s.model.get("sync_frame") >= 96);
    let output = serial.borrow();
    assert!(output.samples.len() > 150);
    assert!(
        output
            .samples
            .iter()
            .rev()
            .take(48)
            .all(|&sample| sample == 0x12_3456)
    );
    drop(output);
    release_write(&mut s);
    assert_eq!(finish_request(&mut s, 91), [0; 12]);
    assert_eq!(read(&mut s, REG_AUDIO_XRUNS), 0);
}
