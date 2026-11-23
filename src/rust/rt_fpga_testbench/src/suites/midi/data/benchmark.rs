//! Full-session playback timing with an independent Rust/output oracle.
mod workload;
use self::workload::*;
use super::{assert_scheduled_jobs, packed};
use crate::{
    Result,
    bits::Bits,
    clock::Edges,
    drivers::{pipeline::Pipeline, store::Geometry},
    memory::{Backend, Control, Memory, Mode, Shared, axi::Axi},
    model::{Model, Probe},
    sim::{Agent, Sim},
    timing::{Fixed, SharedPort, Timing, dram::Dram},
};
use rt_fpga_reference::{
    midi::{InputEvent, Message, OutputEvent},
    transitions::{LoopKind, LoopSpec},
};
use serde_json::{Value, json};
use std::{cell::RefCell, path::Path, rc::Rc};

const FABRIC_HZ: u64 = 100_000_000;
const AUDIO_BUSY: &str = "core.audio.core.pipeline.sample_busy";
const MIDI_BUSY: &str = "core.audio.core.pipeline.midi_lane_busy";
const AUDIO_QUEUED: &str = "core.audio.core.pipeline.audio_queued_jobs";
const MIDI_QUEUED: &str = "core.audio.core.pipeline.midi_queued_jobs";
const SP_VALID: &str = "core.audio.core.pipeline.state_engine.engine_job_valid";
const SP_READY: &str = "core.audio.core.pipeline.state_engine.engine_job_ready";

#[derive(Default)]
struct LaneActivity {
    active: bool,
    audio_busy: u64,
    midi_busy: u64,
    overlap: u64,
    sp_stalled: u64,
    audio_peak: u64,
    midi_peak: u64,
}

impl Agent for LaneActivity {
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if self.active && e.rises(0) {
            let audio = m.get(AUDIO_BUSY);
            let midi = m.get(MIDI_BUSY);
            self.audio_busy += audio;
            self.midi_busy += midi;
            self.overlap += audio & midi;
            self.sp_stalled += m.get(SP_VALID) & (1 - m.get(SP_READY));
            self.audio_peak = self.audio_peak.max(m.get(AUDIO_QUEUED));
            self.midi_peak = self.midi_peak.max(m.get(MIDI_QUEUED));
        }
    }
}

pub struct JobOptions<'a> {
    pub merge_output: bool,
    pub interleave: bool,
    pub shared_ddr: Option<&'a Path>,
}

impl Default for JobOptions<'_> {
    fn default() -> Self {
        Self {
            merge_output: true,
            interleave: true,
            shared_ddr: None,
        }
    }
}

#[derive(Default)]
struct Output(Vec<OutputEvent>);
impl Agent for Output {
    fn drive(&mut self, m: &mut Model<'_>, e: Edges) {
        if e.rises(0) {
            m.set_all(&[("midi_packet_ready", 1), ("midi_packet_refused", 0)]);
        }
    }
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if e.rises(0) && m.get("reset") == 0 && m.get("midi_packet_valid") != 0 {
            let data = (m.get("midi_packet_data") as u32).to_le_bytes();
            self.0.push(OutputEvent {
                destination: m.get("midi_packet_destination") as u16,
                offset: m.get("midi_packet_offset") as u32,
                message: Message::parse(&data[..m.get("midi_packet_length") as usize]).unwrap(),
            });
        }
    }
}
fn control(sim: &mut Sim<'_>, op: u64, args: [u64; 5]) -> [u64; 4] {
    sim.model.set("midi_command_op", op);
    for (i, a) in args.into_iter().enumerate() {
        sim.model.set(&format!("midi_command_arg{i}"), a);
    }
    sim.model
        .set_all(&[("midi_command_valid", 1), ("midi_response_ready", 0)]);
    sim.until(2_000_000, "benchmark control admission", |m| {
        m.get("midi_command_ready") != 0
    });
    sim.idle(1);
    sim.model.set("midi_command_valid", 0);
    sim.until(10_000_000, "benchmark control response", |m| {
        m.get("midi_response_valid") != 0
    });
    assert_eq!(sim.model.get("midi_response_error"), 0, "command {op}");
    let mut result = ["handle", "frame", "packet", "count"]
        .map(|n| sim.model.get(&format!("midi_response_{n}")));
    result[2] |= sim.model.get("midi_response_length") << 24;
    sim.model.set("midi_response_ready", 1);
    sim.idle(1);
    sim.model.set("midi_response_ready", 0);
    result
}
fn rows(driver: &mut Pipeline, sim: &mut Sim<'_>, reference: &Reference, all: bool) {
    for slot in 0..reference.loops {
        let state = reference.pipeline.state().loop_state(id(slot)).unwrap();
        let spec = LoopSpec {
            id: id(slot),
            kind: LoopKind::Primitive,
            length: reference.frames as u64,
            sync_source: None,
            repeat_sync: false,
            timeline: None,
        };
        let image = crate::loop_image::image(state, &spec, None);
        if all {
            crate::loop_image::write_words(
                &mut driver.audio.program,
                sim,
                slot as u32,
                &image,
                0,
                16,
            );
        } else {
            crate::loop_image::write_plans(&mut driver.audio.program, sim, slot as u32, &image);
        }
    }
}
fn load_audio(driver: &mut Pipeline, sim: &mut Sim<'_>, reference: &Reference) {
    driver.store.reset(sim);
    let snapshot = reference.audio.snapshot();
    let blocks = snapshot.samples.len() / QUANTUM / 32;
    if let Backend::Axi(memory) = &mut driver.store.memory.borrow_mut().backend {
        for (target, value) in memory.words.iter_mut().zip(&snapshot.samples) {
            *target = value.0 as u32;
        }
    } else {
        panic!("benchmark needs AXI memory");
    }
    driver
        .store
        .write_config(sim, 0, 3, driver.store.geometry.blocks());
    for (index, view) in snapshot.views.iter().enumerate() {
        let view = view.as_ref().unwrap();
        for (ordinal, chunk) in view.chunks.iter().step_by(32).enumerate() {
            let address = driver.store.table_address(index as u64, ordinal as u64);
            driver
                .store
                .write_config(sim, 2, address, (chunk.0 / 32) as u64);
        }
        let mut descriptor = Bits::zero(160);
        descriptor.set(0, 1, 1).set(64, 64, view.frames).set(
            128,
            32,
            view.chunks.len().div_ceil(32) as u64,
        );
        driver
            .store
            .configure(sim, 1, index as u64, Some(&descriptor), false);
    }
    for block in 0..blocks {
        driver.store.write_config(sim, 3, block as u64, 1);
    }
    for (i, value) in [snapshot.views.len(), blocks, 0].into_iter().enumerate() {
        driver.store.write_config(sim, 0, i as u64, value as u64);
    }
    driver.store.memory.borrow_mut().control.stalls = false;
}
fn load_midi(sim: &mut Sim<'_>, memory: &Shared, reference: &Reference) {
    let limits = limits();
    let entries = limits.max_events.div_ceil(limits.block_events) as usize + 1;
    let table = limits.views as usize * 64;
    let event = table + limits.views as usize * entries * 4 + limits.blocks as usize * 8;
    let bytes = (event
        + limits.blocks as usize * limits.block_events as usize * 16
        + limits.retirements as usize * 8
        + 15)
        & !15;
    assert_eq!(
        sim.model.get("midi_arena_bytes") as usize,
        bytes + 131072 * 16 + (1 + 8 + 256 + 1) * 32768 + (64 * 8 + 1) * 2048
    );
    control(sim, 0, [0; 5]);
    if !reference.has_midi {
        return;
    }
    if let Some(state) = &reference.midi.expression {
        for output in 0..8 {
            for field in 0..8192 {
                let value = state.output(output).unwrap().word(field);
                if value != 0 {
                    control(
                        sim,
                        22,
                        [1, u64::from(output), field as u64, u64::from(value), 0],
                    );
                }
            }
        }
    }
    let helper = control(sim, 8, [0; 5])[0];
    for slot in 0..reference.loops {
        let view = control(sim, 8, [0; 5])[0];
        let view_slot = view as u32 as usize;
        if reference.state_enabled {
            let source = reference
                .midi
                .view(rt_fpga_reference::midi::SequenceId(slot as u16))
                .unwrap()
                .unwrap();
            let snapshot = reference.midi.store.snapshot(source).unwrap();
            for field in 0..8192 {
                let value = snapshot.word(field);
                if value != 0 {
                    control(sim, 22, [2, view, field as u64, u64::from(value), 0]);
                }
            }
        }
        let records = reference.records(slot);
        for (ordinal, chunk) in records.chunks(limits.block_events as usize).enumerate() {
            let first = chunk[0];
            control(
                sim,
                9,
                [view, first.frame, u64::from(packed(first.message)), 0, 0],
            );
            {
                let mut memory = memory.borrow_mut();
                let Backend::Axi(bus) = &mut memory.backend else {
                    unreachable!()
                };
                let block = bus.words[table / 4 + view_slot * entries + ordinal] as usize;
                let start = event / 4 + block * limits.block_events as usize * 4;
                for (offset, record) in chunk.iter().enumerate() {
                    bus.words[start + offset * 4..start + offset * 4 + 4].copy_from_slice(&[
                        record.frame as u32,
                        (record.frame >> 32) as u32,
                        record.order,
                        packed(record.message),
                    ]);
                }
                let count = ordinal * limits.block_events as usize + chunk.len();
                bus.words[view_slot * 16 + 5] = count as u32;
                bus.words[view_slot * 16 + 12] = count as u32;
            }
            // Changing view reloads the store's working descriptor after host preparation.
            control(sim, 12, [helper, 0, 0, 0, 0]);
        }
        control(sim, 10, [view, reference.frames as u64, 0, 0, 0]);
        for index in [0, records.len() / 2, records.len() - 1] {
            let result = control(sim, 12, [view, index as u64, 0, 0, 0]);
            assert_eq!(
                (result[1], result[2], result[3]),
                (
                    records[index].frame,
                    u64::from(packed(records[index].message)),
                    records[index].order as u64
                )
            );
        }
        control(sim, 5, [slot as u64, view, 0, 0, 0]);
        control(sim, 13, [view, 0, 0, 0, 0]);
        if reference.state_enabled {
            control(
                sim,
                21,
                [
                    slot as u64,
                    (1 << 32) | slot as u64,
                    ((1u64 << (slot / 8)) << 8) | ((1u64 << (slot % 8)) << 16),
                    0,
                    0,
                ],
            );
        }
        eprintln!("Prepared RTL MIDI loop {}/{}", slot + 1, reference.loops);
    }
    control(sim, 13, [helper, 0, 0, 0, 0]);
    control(sim, 20, [64, 0, 0, 0, 0]);
}
fn traffic(memory: &Shared) -> [u64; 4] {
    let memory = memory.borrow();
    let Backend::Axi(bus) = &memory.backend else {
        unreachable!()
    };
    [bus.reads, bus.writes, bus.native_reads, bus.native_writes]
}
fn cache_traffic(model: &Model<'_>) -> [[u64; 4]; 3] {
    std::array::from_fn(|bank| {
        ["hits", "misses", "fills", "writebacks"].map(|counter| {
            model.get(&format!(
                "core.audio.core.media.metadata.banks[{bank}].cache.{counter}"
            ))
        })
    })
}

pub fn run(
    audio: bool,
    midi: bool,
    loops: usize,
    frames: usize,
    quanta: usize,
    delay: u64,
) -> Result<Value> {
    run_config(audio, midi, loops, frames, quanta, delay, false)
}
pub fn run_config(
    audio: bool,
    midi: bool,
    loops: usize,
    frames: usize,
    quanta: usize,
    delay: u64,
    state: bool,
) -> Result<Value> {
    run_parameters(
        audio,
        midi,
        loops,
        frames,
        quanta,
        delay,
        state,
        &Default::default(),
    )
}
pub fn run_parameters(
    audio: bool,
    midi: bool,
    loops: usize,
    frames: usize,
    quanta: usize,
    delay: u64,
    state: bool,
    parameters: &std::collections::BTreeMap<String, String>,
) -> Result<Value> {
    run_case(
        audio,
        midi,
        loops,
        frames,
        quanta,
        delay,
        state,
        false,
        false,
        JobOptions::default(),
        parameters,
    )
}
pub fn tracks(
    audio: bool,
    frames: usize,
    quanta: usize,
    delay: u64,
    options: JobOptions<'_>,
    parameters: &std::collections::BTreeMap<String, String>,
) -> Result<Value> {
    assert!(quanta >= 8);
    run_case(
        audio, true, 64, frames, quanta, delay, false, false, true, options, parameters,
    )
}
pub fn musical(delay: u64) -> Result<Value> {
    run_case(
        false,
        true,
        64,
        1_440_000,
        16,
        delay,
        true,
        true,
        false,
        JobOptions::default(),
        &Default::default(),
    )
}
fn run_case(
    audio: bool,
    midi: bool,
    loops: usize,
    frames: usize,
    quanta: usize,
    delay: u64,
    state: bool,
    musical: bool,
    tracks: bool,
    options: JobOptions<'_>,
    parameters: &std::collections::BTreeMap<String, String>,
) -> Result<Value> {
    assert!(quanta > 0 && (quanta + 17) * QUANTUM <= frames);
    let cap = [2048, 256, 2048, QUANTUM as u64, 512, 2, 2];
    let mut geometry = Geometry::new([512, 5_763_072, QUANTUM as u64], 32);
    geometry.reuse_views = true;
    geometry.table_lines = 128;
    let mut design = crate::suites::pipeline::design(cap, &geometry, 64, Mode::Fixed, 1);
    design.top = "midi_audio_pipeline_axi".into();
    design.probes.clear();
    for signal in [
        AUDIO_BUSY,
        MIDI_BUSY,
        AUDIO_QUEUED,
        MIDI_QUEUED,
        SP_VALID,
        SP_READY,
    ] {
        design.probes.push(Probe::observe(signal));
    }
    for bank in 0..3 {
        for counter in ["hits", "misses", "fills", "writebacks"] {
            design.probes.push(Probe::observe(&format!(
                "core.audio.core.media.metadata.banks[{bank}].cache.{counter}"
            )));
        }
    }
    design.includes.push(crate::root().join("fpga/midi/rtl"));
    for name in [
        "midi_audio_pipeline_axi",
        "midi_audio_pipeline",
        "midi_data_jobs",
        "midi_expression_state",
        "midi_store",
    ] {
        design
            .sources
            .push(crate::root().join(format!("fpga/midi/rtl/{name}.sv")));
    }
    for (name, value) in [
        ("MIDI_STATE_SUPPORTED", 1),
        ("MIDI_LOOPS", 64),
        ("MIDI_INPUTS", 1),
        ("MIDI_DESTINATIONS", 8),
        ("MIDI_BUFFERS", 256),
        ("MIDI_CURSORS", 64),
        ("MIDI_SCRATCH_EVENTS", 131072),
        ("MIDI_BLOCK_EVENTS", 256),
        ("MIDI_BLOCKS", 2432),
        ("MIDI_MAX_EVENTS", 8192),
        ("MIDI_VIEWS", 256),
        ("MIDI_RETIREMENTS", 128),
        ("MIDI_MAINTENANCE", 64),
        ("MIDI_RECORD_BURSTS", 1),
        ("SPLIT_MIDI_JOBS", 1),
        ("MIDI_JOB_FIFO_DEPTH", 128),
        ("MIDI_AXI_DATA_WIDTH", 64),
        ("READ_HINT_DEPTH", 256),
    ] {
        design.parameters.insert(name.into(), value.to_string());
    }
    if std::fs::read_to_string(crate::root().join("fpga/midi/rtl/midi_audio_pipeline_axi.sv"))?
        .contains("parameter integer DDR_PROGRAM")
    {
        design.parameters.insert("DDR_PROGRAM".into(), "1".into());
    }
    for (name, value) in parameters {
        if !design.parameters.contains_key(name)
            && !matches!(name.as_str(), "READ_HINT_POLICY" | "LOCAL_FRAME_BITS")
        {
            return Err(format!("Unknown benchmark parameter: {name}").into());
        }
        if name == "READ_HINT_POLICY" && !matches!(value.as_str(), "0" | "1" | "2") {
            return Err("READ_HINT_POLICY must be 0, 1 or 2".into());
        }
        if name == "LOCAL_FRAME_BITS" {
            let bits: u32 = value.parse()?;
            if !(1..=64).contains(&bits) {
                return Err("LOCAL_FRAME_BITS must be between 1 and 64".into());
            }
            if audio && bits < 64 && frames as u64 >= (1u64 << bits) {
                return Err("Audio preload exceeds the explicit local-frame limit".into());
            }
        }
        design.parameters.insert(name.clone(), value.clone());
    }
    let model_parameters = design.parameters.clone();
    let compiled = design.build(&crate::root().join("work/marlin-suite/midi-data-benchmark"))?;
    let mut sim = Sim::new(compiled.benchmark_session()?, &[("clk", 10_000, 0)]);
    sim.max_ps = 10_000_000_000_000_000;
    let controller = if let Some(output) = options.shared_ddr {
        let root = crate::root();
        let config = std::env::var_os("SHOOP_DRAMSIM3_CONFIG")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| root.join("fpga/axi_memory/configs/zturn-v2-ddr3-1066-x32.ini"));
        let dram: Box<dyn Timing> = Box::new(Dram::new(
            &root.join("work/dramsim3/build/libshoop_dram.so"),
            &config,
            output,
            FABRIC_HZ,
        )?);
        Some(Rc::new(RefCell::new(dram)))
    } else {
        None
    };
    let port = |advances_clock| -> Box<dyn Timing> {
        if let Some(controller) = &controller {
            Box::new(SharedPort {
                controller: controller.clone(),
                advances_clock,
            })
        } else {
            Box::new(Fixed::new(delay))
        }
    };
    let size = sim.model.get("midi_arena_bytes") as usize + 32768;
    let midi_memory = Rc::new(RefCell::new(Memory {
        control: Control::default(),
        backend: Backend::Axi(Box::new(
            Axi::new(size / 4, 0x30000000, 8, port(false), 119).with_prefix("midi_"),
        )),
        words: size / 4,
        base: 0x30000000,
        width: 8,
        sample_words: None,
        sample_transactions: 0,
    }));
    sim.agent(midi_memory.clone());
    let output = Rc::new(RefCell::new(Output::default()));
    sim.agent(output.clone());
    let activity = Rc::new(RefCell::new(LaneActivity::default()));
    sim.agent(activity.clone());
    let mut driver = Pipeline::new(&mut sim, cap, geometry, Mode::Fixed, 1021)?;
    if model_parameters
        .get("DDR_PROGRAM")
        .is_some_and(|v| v != "0")
    {
        driver.store.reserve_ddr_program(
            model_parameters["PROGRAM_DEPTH"].parse()?,
            model_parameters
                .get("PROGRAM_REGIONS")
                .map_or(Ok(1), |v| v.parse())?,
        );
    }
    if let Backend::Axi(memory) = &mut driver.store.memory.borrow_mut().backend {
        memory.timing = port(true);
    }
    let mut reference = Reference::prepare_profile(audio, midi, loops, frames, state, tracks);
    if musical {
        reference.musical_fixture();
    }
    let views: Vec<_> = (0..if audio { loops * 2 } else { 0 })
        .map(|i| i as u64)
        .collect();
    let image = crate::target::RtlTarget::new(cap, 256, &views).build_with_midi_options(
        &reference.logical,
        options.merge_output,
        options.interleave,
    )?;
    driver.audio.load_compiled(&mut sim, &image);
    let event_address = (size - 32768 + 271) & !15;
    sim.model.set_all(&[
        ("run_quantum", QUANTUM as u64),
        ("sync_event_ready", 1),
        ("midi_arena_base", 0),
        ("midi_memory_arena_bytes", size as u64),
        ("midi_event_count", 0),
        ("midi_event_address", event_address as u64),
        ("midi_publication_commit", 1),
    ]);
    load_audio(&mut driver, &mut sim, &reference);
    for channel in 0..if audio { 2 } else { 0 } {
        for offset in 0..QUANTUM {
            driver.audio.sample_config(
                &mut sim,
                0,
                (channel * QUANTUM + offset) as u64,
                Some(0),
                false,
            );
        }
    }
    rows(&mut driver, &mut sim, &reference, true);
    load_midi(&mut sim, &midi_memory, &reference);
    sim.model.set("midi_publication_commit", 0);
    reference.play();
    rows(&mut driver, &mut sim, &reference, false);
    let mut measurements = Vec::new();
    let steady_end = quanta + 17;
    for tick in 0..steady_end
        + if musical {
            7
        } else if tracks {
            3
        } else {
            0
        }
    {
        use rt_fpga_reference::transitions::Operation;
        let label = if musical && tick >= quanta + 17 {
            match tick - (quanta + 17) {
                0 => {
                    reference.command(Operation::Play {
                        position: 0,
                        retrigger: true,
                    });
                    "retrigger"
                }
                1 => {
                    reference.command(Operation::Stop);
                    "stop_cleanup"
                }
                2 => {
                    reference.command(Operation::Play {
                        position: 0,
                        retrigger: true,
                    });
                    "warm_play"
                }
                3 => {
                    reference.command(Operation::Seek {
                        position: (frames - QUANTUM) as u64,
                        retrigger: false,
                    });
                    "seek_prefix"
                }
                4 => "wrap_after_seek",
                5 => {
                    reference.command(Operation::Stop);
                    "stop_after_wrap"
                }
                _ => "quiet_after_stop",
            }
        } else if tracks && tick == 20 {
            reference.command_slots(0..8, Operation::Stop);
            "stop_eight"
        } else if tracks && tick == 22 {
            reference.command_slots(
                0..8,
                Operation::Play {
                    position: (tick * QUANTUM) as u64,
                    retrigger: true,
                },
            );
            "start_eight"
        } else if tracks && tick == steady_end {
            reference.command(Operation::Record);
            "record_start"
        } else if tracks && tick == steady_end + 1 {
            "record_eight_events"
        } else if tracks && tick == steady_end + 2 {
            reference.command(Operation::Stop);
            "record_stop"
        } else if tick == 0 {
            "playback_start"
        } else if tick < 17 {
            "warmup"
        } else {
            "steady"
        };
        if (musical && tick >= steady_end)
            || (tracks
                && matches!(
                    label,
                    "stop_eight" | "start_eight" | "record_start" | "record_stop"
                ))
        {
            rows(&mut driver, &mut sim, &reference, false);
        }
        let mut input = Vec::new();
        if tracks && (steady_end..steady_end + 2).contains(&tick) {
            let mut memory = midi_memory.borrow_mut();
            let Backend::Axi(bus) = &mut memory.backend else {
                unreachable!()
            };
            for i in 0..8 {
                let event = InputEvent {
                    input: 0,
                    offset: (i * 4) as u32,
                    message: Message::parse(&[0xb0, 7, 30 + i as u8]).unwrap(),
                };
                let start = event_address / 4 + 4 * i;
                bus.words[start..start + 4].copy_from_slice(&[
                    u32::from(event.input),
                    event.offset,
                    packed(event.message),
                    0,
                ]);
                input.push(event);
            }
        }
        sim.model.set("midi_event_count", input.len() as u64);
        reference.callback_with_input(&input);
        if tracks {
            reference.validate_track_playback();
        } else if !musical {
            reference.validate();
        }
        driver.audio.clear_jobs();
        let audio_before = traffic(&driver.store.memory);
        let cache_before = cache_traffic(&sim.model);
        let midi_before = traffic(&midi_memory);
        sim.model.set_all(&[
            ("run_buffers", driver.audio.buffers),
            ("run_inputs", if audio { 2 } else { 0 }),
            ("run_outputs", driver.audio.outputs),
            ("job_enable", 1),
        ]);
        *activity.borrow_mut() = LaneActivity {
            active: true,
            ..Default::default()
        };
        driver.audio.start(&mut sim);
        let start = sim.cycles - 1;
        sim.until(20_000_000, "64-loop MIDI data-job completion", |m| {
            m.get("done") != 0
        });
        let cycles = sim.cycles - start;
        activity.borrow_mut().active = false;
        assert_eq!(sim.model.get("error_code"), 0);
        assert_eq!(sim.model.get("midi_error_code"), 0);
        assert_eq!(
            std::mem::take(&mut output.borrow_mut().0),
            reference.emitted
        );
        assert_scheduled_jobs(driver.audio.consumed(), reference.pipeline.emitted_jobs());
        assert_eq!(
            sim.model.get("sync_frame"),
            reference.pipeline.state().frame()
        );
        let audio_traffic =
            std::array::from_fn::<_, 4, _>(|i| traffic(&driver.store.memory)[i] - audio_before[i]);
        let midi_traffic =
            std::array::from_fn::<_, 4, _>(|i| traffic(&midi_memory)[i] - midi_before[i]);
        let cache_after = cache_traffic(&sim.model);
        let cache_delta = std::array::from_fn::<_, 3, _>(|bank| {
            std::array::from_fn::<_, 4, _>(|counter| {
                cache_after[bank][counter] - cache_before[bank][counter]
            })
        });
        sim.idle(2);
        for channel in 0..reference.outputs.len() {
            for offset in 0..QUANTUM {
                let actual = driver.audio.sample_config(
                    &mut sim,
                    1,
                    (channel * QUANTUM + offset) as u64,
                    None,
                    false,
                );
                assert_eq!(actual, reference.outputs[channel][offset].0);
            }
        }
        for slot in 0..loops {
            let actual =
                crate::loop_image::read_loop(&mut driver.audio.program, &mut sim, slot as u32);
            let expected = reference.pipeline.state().loop_state(id(slot)).unwrap();
            assert_eq!(actual.get(33, 2), expected.state.mode as u64);
            assert_eq!(actual.get(128, 64), expected.state.position);
            assert_eq!(actual.get(192, 64), expected.state.content_length);
            assert_eq!(actual.get(512, 64), expected.render.position);
        }
        if !audio {
            assert_eq!(audio_traffic, [0; 4]);
        }
        if !midi {
            assert!(reference.emitted.is_empty());
        }
        let lane = activity.borrow();
        measurements.push(json!({
            "kind":label,
            "position":reference.position(), "cycles":cycles, "us_at_100mhz":cycles as f64/100.0,
            "midi_input_packets":input.len(), "midi_packets":reference.emitted.len(), "jobs":driver.audio.consumed().len(),
            "state_work":reference.midi.expression.as_ref().map(|s|json!({"tracker_visits":s.work.tracker_visits,"compared_fields":s.work.compared_fields,"restored_messages":s.work.restored_messages,"prefix_events":s.work.prefix_events})),
            "audio_axi_reads_writes_native_reads_writes":audio_traffic,
            "audio_metadata_table_reference_stack_hits_misses_fills_writebacks":cache_delta,
            "midi_axi_reads_writes_native_reads_writes":midi_traffic,
            "job_activity":{"sample_engine_busy_cycles":lane.audio_busy,"midi_lane_busy_cycles":lane.midi_busy,
                "overlap_cycles":lane.overlap,"sp_job_backpressure_cycles":lane.sp_stalled,
                "audio_or_mixed_queue_peak":lane.audio_peak,"midi_queue_peak":lane.midi_peak},
        }));
        eprintln!("RTL audio={audio} midi={midi}, delay={delay}, quantum {tick}: {cycles} clocks");
    }
    Ok(json!({
        "scenario":if audio && midi {"stereo_and_midi"} else if audio {"audio_only"} else {"midi_only"},
        "loops":loops, "sample_rate":SAMPLE_RATE, "quantum_frames":QUANTUM, "chunk_frames":QUANTUM,
        "sequence_frames":frames, "midi_events_per_loop":(!tracks).then(|| if midi {records(0,frames).len()} else {0}),
        "ddr_added_delay_clocks":options.shared_ddr.is_none().then_some(delay),
        "rtl_parameters":model_parameters,
        "simulation_optimization":3, "waveform_instrumentation":false,
        "memory":if options.shared_ddr.is_some() {"two 64-bit AXI ports sharing one DRAMsim3 DDR3 controller".into()} else {format!("two independent 64-bit AXI ports, fixed {delay}-clock service per 16-byte native request, no random bus stalls; AXI handshakes retained; excludes board DDR arbitration/row/refresh effects")},
        "shared_ddr":controller.map(|c| c.borrow_mut().report(true)),
        "scope":"full compiled SP and job execution from acceptance through done, including input validation and cleanup; preparation and readback excluded",
        "preparation":"RTL allocates view/block metadata; host loads event payloads/counts outside timing and reloads working descriptors; first/middle/last records checked by public export reads",
        "compiled_instructions":image.instructions.len(), "maximum_jobs":image.jobs_per_quantum,
        "validation":if musical {"every SP job, MIDI message, sync frame and loop position/length matches Rust; note/pedal meaning checked by independent receiver fixtures"} else if tracks {"every SP job, MIDI message, stereo sample, sync frame and all loop positions/lengths match Rust; playback MIDI independently checked against fixture records"} else {"every SP job, MIDI message, stereo sample, sync frame and all loop positions/lengths match Rust; sample/event formulas independently checked"},
        "musical_fixture":musical,"tracks_fixture":tracks,"merge_output":options.merge_output,"interleave_jobs":options.interleave,
        "midi_state_hardware":true, "midi_state_tracking":state&&midi, "measurements":measurements,
    }))
}

pub fn cpu(audio: bool, midi: bool, state: bool, samples: usize) -> Value {
    let mut r = Reference::prepare(audio, midi, 64, 1_440_000, state);
    r.pipeline.trace_jobs(false);
    r.play();
    let t = std::time::Instant::now();
    r.callback();
    let startup = t.elapsed().as_nanos() as u64;
    r.validate();
    for _ in 0..16 {
        r.callback();
        r.validate();
    }
    let mut timings = Vec::with_capacity(samples);
    let mut messages = 0;
    for _ in 0..samples {
        let t = std::time::Instant::now();
        r.callback();
        timings.push(t.elapsed().as_nanos() as u64);
        r.validate();
        messages += r.emitted.len();
    }
    let mut sorted = timings.clone();
    sorted.sort_unstable();
    json!({"audio":audio,"midi":midi,"state":state&&midi,"loops":64,"frames":1440000,"quantum":32,"sample_rate":48000,
        "samples_ns":timings,"startup_ns":startup,"median_ns":sorted[samples/2],"p99_ns":sorted[(samples-1)*99/100],"max_ns":sorted[samples-1],
        "messages":messages,"validation":"each completed callback independently checks MIDI, audio, loop position; checks outside timer; prepared engine, tracing disabled",
        "cpu":std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default().lines().find(|line|line.starts_with("model name")).unwrap_or("unknown"),
        "os":std::env::consts::OS,"architecture":std::env::consts::ARCH})
}
