use super::{Output, control};
use crate::{
    Result,
    bits::Bits,
    clock::Edges,
    drivers::{pipeline::Pipeline, store::Geometry},
    memory::{Backend, Control, Mode, axi::Axi},
    model::Model,
    sim::{Agent, Sim},
    timing::Fixed,
};
use rt_fpga_compiler::StateProgramTarget;
use rt_fpga_reference::{
    midi::MidiState,
    transitions::{LoopKind, LoopSpec},
};
use std::{cell::RefCell, rc::Rc};

#[path = "../../../../../rt_fpga_reference/examples/support/session_64.rs"]
mod session;
use session::*;

struct MidiMemory {
    bus: Axi,
    control: Control,
}
impl Agent for MidiMemory {
    fn drive(&mut self, model: &mut Model<'_>, edges: Edges) {
        if edges.rises(0) {
            self.bus.drive(model, &self.control);
        }
    }
    fn sample(&mut self, model: &Model<'_>, edges: Edges) {
        if edges.rises(0) {
            self.bus.sample(model, &mut self.control);
        }
    }
}

fn load_audio(driver: &mut Pipeline, sim: &mut Sim<'_>, reference: &Reference) {
    driver.store.reset(sim);
    let snapshot = reference.audio.snapshot();
    let slots = snapshot.samples.len() / QUANTUM;
    let blocks = slots / 32;
    if let Backend::Axi(memory) = &mut driver.store.memory.borrow_mut().backend {
        for (target, value) in memory.words.iter_mut().zip(&snapshot.samples) {
            *target = value.0 as u32;
        }
    } else {
        panic!("benchmark requires the external AXI memory model");
    }
    driver
        .store
        .write_config(sim, 0, 3, driver.store.geometry.blocks());
    for (index, view) in snapshot.views.iter().enumerate() {
        let view = view.as_ref().unwrap();
        for (ordinal, chunk) in view.chunks.iter().step_by(32).enumerate() {
            driver.store.write_config(
                sim,
                2,
                driver.store.table_address(index as u64, ordinal as u64),
                (chunk.0 / 32) as u64,
            );
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
    for (index, value) in [snapshot.views.len(), blocks, 0].into_iter().enumerate() {
        driver
            .store
            .write_config(sim, 0, index as u64, value as u64);
    }
    driver.store.memory.borrow_mut().control.stalls = false;
}

struct MidiLayout {
    table: usize,
    snapshot: usize,
    event: usize,
    output: usize,
    map: usize,
    directory: usize,
    bytes: usize,
}
impl MidiLayout {
    fn new(loops: usize) -> Self {
        let l = limits(loops);
        let s = l.store;
        let table = s.views as usize * 64;
        let snapshot = table + s.views as usize * 35 * 4 + s.blocks as usize * 8;
        let event = snapshot + s.snapshots as usize * 16916;
        let content =
            event + s.blocks as usize * s.block_events as usize * 16 + s.retirements as usize * 8;
        let input = content + loops * 128;
        let output = input + 16900;
        let history = output + DESTINATIONS * 16900 + loops * 16900;
        let map = history + 96 + s.snapshots as usize * 16 + 532;
        let directory = map + s.snapshots as usize * DESTINATIONS * 532;
        Self {
            table,
            snapshot,
            event,
            output,
            map,
            directory,
            bytes: directory + s.snapshots as usize * DESTINATIONS * 48 + 532,
        }
    }
    fn prepare_maps(&self, memory: &mut [u32], snapshot: u64) {
        let slot = snapshot as u32 as usize;
        let header = self.snapshot + slot * 16916;
        for destination in 0..DESTINATIONS {
            let pair = slot * DESTINATIONS + destination;
            let map = self.map + pair * 532;
            memory[map / 4..map / 4 + 133].fill(0);
            let output = self.output + destination * 16900;
            let address = self.directory / 4 + pair * 12;
            memory[address..address + 12].copy_from_slice(&[
                1,
                0,
                (header + 16) as u32,
                0,
                output as u32,
                0,
                map as u32,
                0,
                header as u32,
                0,
                (snapshot >> 32) as u32,
                4225,
            ]);
        }
    }
}

fn load_midi(sim: &mut Sim<'_>, memory: &Rc<RefCell<MidiMemory>>, loops: usize, frames: usize) {
    let layout = MidiLayout::new(loops);
    assert_eq!(sim.model.get("midi_arena_bytes") as usize, layout.bytes);
    let scratch = layout.bytes.div_ceil(16) * 16 + 256;
    let initial = super::super::projection(&MidiState::default());
    memory.borrow_mut().bus.words[scratch / 4..scratch / 4 + initial.len()]
        .copy_from_slice(&initial);
    assert_eq!(control(sim, 0, [0; 5]), 0);
    assert_eq!(control(sim, 16, [0, 0, 0, 0, 0]), 0);
    sim.model.set("midi_publication_commit", 1);
    for slot in 0..loops {
        assert_eq!(
            control(
                sim,
                1,
                [
                    slot as u64,
                    (1u64 << 32) | slot as u64,
                    0,
                    (slot / 8) as u64,
                    1 << (slot % 8)
                ]
            ),
            0
        );
        assert_eq!(control(sim, 8, [scratch as u64, 0, 0, 0, 0]), 0);
        let view = sim.model.get("midi_response_handle");
        let view_slot = view as u32 as usize;
        let descriptor = view_slot * 16;
        let records = records(slot, frames);
        for (ordinal, chunk) in records.chunks(256).enumerate() {
            let first = chunk[0];
            let bytes = first.message.as_bytes();
            let packet = (3u64 << 24)
                | u64::from(bytes[0])
                | u64::from(bytes[1]) << 8
                | u64::from(bytes[2]) << 16;
            assert_eq!(control(sim, 9, [view, first.frame, packet, 0, 0]), 0);
            let mut memory = memory.borrow_mut();
            let words = &mut memory.bus.words;
            let block = words[layout.table / 4 + view_slot * 35 + ordinal] as usize;
            let start = layout.event / 4 + block * 256 * 4;
            for (offset, record) in chunk.iter().enumerate() {
                let bytes = record.message.as_bytes();
                words[start + offset * 4..start + offset * 4 + 4].copy_from_slice(&[
                    record.frame as u32,
                    (record.frame >> 32) as u32,
                    record.order,
                    (3 << 24)
                        | u32::from(bytes[0])
                        | u32::from(bytes[1]) << 8
                        | u32::from(bytes[2]) << 16,
                ]);
            }
            let count = ordinal * 256 + chunk.len();
            words[descriptor + 5] = count as u32;
            words[descriptor + 12] = count as u32;
        }
        {
            let mut memory = memory.borrow_mut();
            let words = &mut memory.bus.words;
            let snapshot =
                u64::from(words[descriptor + 10]) | u64::from(words[descriptor + 11]) << 32;
            layout.prepare_maps(words, snapshot);
        }
        assert_eq!(control(sim, 10, [view, frames as u64, 65536, 0, 0]), 0);
        assert_eq!(sim.model.get("midi_response_count"), 1);
        for index in [0, records.len() / 2, records.len() - 1] {
            assert_eq!(control(sim, 12, [view, index as u64, 0, 0, 0]), 0);
            let expected = records[index];
            assert_eq!(sim.model.get("midi_response_frame"), expected.frame);
            let packet = sim.model.get("midi_response_packet");
            assert_eq!(packet.to_le_bytes()[..3], *expected.message.as_bytes());
        }
        assert_eq!(control(sim, 5, [slot as u64, view, 0, 0, 0]), 0);
        assert_eq!(control(sim, 13, [view, 0, 0, 0, 0]), 0);
        eprintln!("RTL prepared MIDI loop {}/{}", slot + 1, loops);
    }
    sim.model.set("midi_publication_commit", 0);
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

pub fn benchmark_64(
    audio: bool,
    loops: usize,
    frames: usize,
    quanta: usize,
) -> Result<serde_json::Value> {
    let cap = [2048, 256, 2048, QUANTUM as u64, 512, 2, 2];
    let mut geometry = Geometry::new([512, 5_763_072, QUANTUM as u64], 32);
    geometry.reuse_views = true;
    geometry.table_lines = 128;
    let mut design = crate::suites::pipeline::design(cap, &geometry, 64, Mode::Fixed, 1);
    design.top = "midi_audio_pipeline_axi".into();
    design.probes.clear();
    design.includes.push(crate::root().join("fpga/midi/rtl"));
    for name in [
        "midi_audio_pipeline_axi",
        "midi_audio_pipeline",
        "midi_job_port",
        "midi_job_service",
        "midi_store",
        "midi_state_service",
        "midi_state",
        "midi_difference",
        "midi_transition",
    ] {
        design
            .sources
            .push(crate::root().join(format!("fpga/midi/rtl/{name}.sv")));
    }
    for (name, value) in [
        ("MIDI_LOOPS", loops as u64),
        ("MIDI_INPUTS", 1),
        ("MIDI_DESTINATIONS", 8),
        ("MIDI_BLOCK_EVENTS", 256),
        ("MIDI_BLOCKS", 2432),
        ("MIDI_MAX_EVENTS", 8192),
        ("MIDI_VIEWS", 256),
        ("MIDI_SNAPSHOTS", 384),
        ("MIDI_RETIREMENTS", 128),
        ("MIDI_INPUT_EVENTS", 16),
        ("MIDI_MAINTENANCE", 64),
        ("MIDI_AXI_DATA_WIDTH", 64),
        ("READ_HINT_DEPTH", 256),
    ] {
        design.parameters.insert(name.into(), value.to_string());
    }
    let compiled = design.build(&crate::root().join("work/marlin-suite/midi-64-benchmark"))?;
    let mut sim = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
    sim.max_ps = 10_000_000_000_000_000;
    let size = sim.model.get("midi_arena_bytes") as usize + 32768;
    let midi_memory = Rc::new(RefCell::new(MidiMemory {
        bus: Axi::new(size / 4, 0x30000000, 8, Box::new(Fixed::new(7)), 119).with_prefix("midi_"),
        control: Control::default(),
    }));
    sim.agent(midi_memory.clone());
    let output = Rc::new(RefCell::new(Output {
        events: Vec::new(),
        remaining: usize::MAX,
    }));
    sim.agent(output.clone());
    let mut driver = Pipeline::new(&mut sim, cap, geometry, Mode::Fixed, 1021)?;
    let mut reference = Reference::new(audio, loops, frames);
    let views = (0..if audio { loops * 2 } else { 0 })
        .map(|i| i as u64)
        .collect::<Vec<_>>();
    let image = crate::target::RtlTarget::new(cap, 256, &views).build(&reference.logical)?;
    driver.audio.load_compiled(&mut sim, &image);
    sim.model.set_all(&[
        ("run_quantum", QUANTUM as u64),
        ("sync_event_ready", 1),
        ("midi_arena_base", 0),
        ("midi_memory_arena_bytes", size as u64),
        ("midi_event_count", 0),
        ("midi_event_address", 0),
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
    load_midi(&mut sim, &midi_memory, loops, frames);
    reference.play();
    rows(&mut driver, &mut sim, &reference, false);
    let mut measurements = Vec::new();
    for tick in 0..quanta + 17 {
        let report = reference.callback();
        assert_eq!(report.error, None);
        reference.validate();
        driver.audio.clear_jobs();
        sim.model.set_all(&[
            ("run_buffers", driver.audio.buffers),
            ("run_inputs", if audio { 2 } else { 0 }),
            ("run_outputs", driver.audio.outputs),
            ("job_enable", 1),
        ]);
        driver.audio.start(&mut sim);
        let start = sim.cycles;
        sim.until(20_000_000, "64-loop quantum completion", |m| {
            m.get("done") != 0
        });
        let cycles = sim.cycles - start + 1;
        assert_eq!(sim.model.get("error_code"), 0);
        assert_eq!(sim.model.get("midi_error_code"), 0);
        assert_eq!(
            std::mem::take(&mut output.borrow_mut().events),
            reference.emitted
        );
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
        if tick == 0 || tick >= 17 {
            measurements.push(serde_json::json!({"kind":if tick==0 {"playback_start"} else {"steady"},"position":reference.position(),"cycles":cycles,"us_at_100mhz":cycles as f64/100.0,"midi_packets":reference.emitted.len()}));
        }
        eprintln!(
            "RTL {} quantum {tick}: {cycles} clocks",
            if audio { "mixed" } else { "MIDI-only" }
        );
    }
    Ok(
        serde_json::json!({"scenario":if audio {"stereo_and_midi"} else {"midi_only"},"loops":loops,"sample_rate":SAMPLE_RATE,"quantum_frames":QUANTUM,"chunk_frames":QUANTUM,"sequence_frames":frames,
        "memory":"two independent 64-bit AXI ports, fixed 7-clock service timing, no random bus stalls; excludes board DDR arbitration",
        "scope":"full compiled SP, mixed queue and common JP; run acceptance through done; host loading/readback excluded",
        "preparation":"payloads and ready default-state comparison maps loaded outside measured quanta; hardware allocates MIDI descriptors/blocks and exported sample records are checked",
        "measurements":measurements}),
    )
}
