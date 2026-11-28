use super::*;
use rt_fpga_compiler::{
    Identity, InitialValue, LogicalStateProgram, Operation, ProgramLimits, Ramp, Resources,
    StateBinding, StateKey, StateRole, loading::Allocator,
};
use rt_fpga_rtl::{
    host::{
        UploadStep,
        semantic::{Preparation, PreparationStep},
    },
    target::{RtlTarget, Space},
};

#[shoop_test(no_wasm = "Full-profile generated-audio semantic preparation/result/snapshot MMIO")]
fn whole_sync_preparation_and_bound_gain_preserve_original_float_in_real_snapshots() {
    let root = rt_fpga_rtl::root();
    let compiled = paced_design()
        .build(&root.join("work/cmd-fifo-results-paced"))
        .unwrap();
    let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
    sim.max_ps = 1_000_000_000_000;
    sim.model.set_all(&[("reset", 1), ("epoch", 1)]);
    sim.idle(40);
    sim.model.set("reset", 0);
    sim.idle(6000);
    let sim = RefCell::new(sim);
    let ops = Rc::new(Cell::new(0));
    let mut device = Device::open(
        Count {
            bus: Axi(&sim),
            ops: ops.clone(),
            resets: Rc::new(Cell::new(0)),
            lose_recovery: Rc::new(Cell::new(false)),
        },
        contract(),
    )
    .unwrap();
    let limits = ProgramLimits {
        instructions: 1024,
        state_values: 512,
        loop_slots: 128,
        jobs: 512,
        buffers: 512,
        history_slots: 512,
        input_ports: 1,
        output_ports: 2,
        quantum: 32,
    };
    let ids = [
        Identity {
            slot: 41,
            generation: 5,
        },
        Identity {
            slot: 101,
            generation: 6,
        },
    ];
    let gains = [0.12345679_f32, -0.0];
    let logical = LogicalStateProgram {
        quantum: 32,
        chunk_frames: 32,
        sync_pass: ids.to_vec(),
        composites: vec![],
        work_pass: vec![
            Operation::Input {
                port: 0,
                destination: 0,
            },
            Operation::Output { source: 0, port: 0 },
            Operation::Output { source: 0, port: 1 },
        ],
        initial_state: gains
            .iter()
            .map(|&gain| {
                let value = (f64::from(gain) * f64::from(1 << 24)) as i32;
                InitialValue::Ramp(Ramp {
                    start: value,
                    target: value,
                    duration: 0,
                    elapsed: 0,
                })
            })
            .collect(),
        state_bindings: ids
            .iter()
            .enumerate()
            .map(|(index, &id)| StateBinding {
                key: StateKey {
                    owner: rt_fpga_compiler::shoop_graph::Owner::Loop(id),
                    role: StateRole::Gain,
                    channel: 0,
                },
                address: index as u32,
            })
            .collect(),
        loops: ids
            .iter()
            .enumerate()
            .map(|(index, &id)| rt_fpga_compiler::Loop {
                id,
                length: 0,
                sync_source: (index == 1).then_some(ids[0]),
                repeat_sync: false,
            })
            .collect(),
        bindings: ids
            .iter()
            .enumerate()
            .map(|(index, &id)| rt_fpga_compiler::LoopBinding {
                id,
                initial_gain_bits: Some(gains[index].to_bits()),
                gain: Some(index as u32),
                view: None,
                meter: None,
            })
            .collect(),
        jobs_per_quantum: 3,
        resources: Resources {
            queue_capacity: 3,
            buffers: 1,
            history_slots: 0,
            input_ports: 1,
            output_ports: 2,
            events: 128,
        },
    };
    let mut allocator = Allocator::new(limits);
    let layout = allocator.stage(&logical).unwrap();
    let image = RtlTarget {
        capacities: limits,
        media_views: &[],
        state_base: 0,
    }
    .build_semantic_image(&logical, layout)
    .unwrap();
    let mut recipe = Preparation::new(image).unwrap();
    let mut tag = 1000;
    while let Some(step) = recipe.next_step() {
        let mut payload = [0; 12];
        let operation = match step {
            PreparationStep::Upload(UploadStep::Configure(write)) => {
                let (service, space) = match write.space {
                    Space::Program => (SEMANTIC_SERVICE_CORE_CONFIG, 0),
                    Space::State => (SEMANTIC_SERVICE_CORE_CONFIG, 1),
                    Space::Loop => (SEMANTIC_SERVICE_CORE_CONFIG, 2),
                    Space::History => (SEMANTIC_SERVICE_AUDIO_CONFIG, 3),
                };
                payload[0] = 0x100 | space;
                payload[1] = u32::try_from(write.address).unwrap();
                payload[2..7].copy_from_slice(&write.value.array::<5>());
                service
            }
            PreparationStep::Upload(UploadStep::ClearView(view)) => {
                payload[0] = view;
                SEMANTIC_SERVICE_CLEAR_VIEW
            }
            PreparationStep::Observation { slot, word, value } => {
                payload[0] = 0x103;
                payload[1] = u32::from(slot) * 3 + u32::from(word);
                payload[2] = value as u32;
                payload[3] = (value >> 32) as u32;
                SEMANTIC_SERVICE_CORE_CONFIG
            }
        };
        // This serial fixture issues genuinely unused preparation, not a trusted lease API.
        service(&sim, operation, payload, tag);
        tag += 1;
        assert_eq!(sim.borrow().model.get("sync_frame"), 0);
        assert_eq!(Axi(&sim).read(REG_COMPLETED_QUANTA), Ok(0));
    }
    device
        .queue(
            GroupPacket::new(
                GroupHeader {
                    epoch: 1,
                    sequence: 1,
                    graph: 0,
                    topology: 0,
                    deadline: None,
                },
                &[CommandRecord::switch(recipe.descriptor(), false)],
            )
            .unwrap(),
        )
        .unwrap();
    publish(&mut device, &ops);
    assert_eq!(sim.borrow().model.get("active_revision"), 0);
    let mut payload = [0; 12];
    payload[0] = 1;
    service(&sim, SEMANTIC_SERVICE_ENABLE_AUDIO, payload, tag);
    sim.borrow_mut().idle(100_000);
    buffer(&mut device, &ops);
    let result = device.take_result().unwrap();
    assert_eq!(result.header().switch, SwitchOutcome::Fenced);
    assert_eq!(result.header().first_frame, 0);
    let mut first = vec![0; SNAPSHOT_COPY_WORDS];
    let mut second = vec![0; SNAPSHOT_COPY_WORDS];
    let mut device = device.with_snapshots(SnapshotBuffers::new(&mut first, &mut second).unwrap());
    let mut updated = false;
    for _ in 0..11_000 {
        let before = ops.get();
        let step = device.snapshot_step().unwrap();
        assert!(ops.get() - before <= MAX_SNAPSHOT_BUS_OPS);
        if matches!(step, SnapshotStep::Updated(_)) {
            updated = true;
            break;
        }
    }
    assert!(updated);
    let snapshot = device.latest_snapshot().unwrap();
    assert_eq!(
        (snapshot.header().frame, snapshot.header().revision),
        (0, 1)
    );
    for (slot, gain) in gains.into_iter().enumerate() {
        let row = snapshot.physical_row(slot).unwrap();
        assert_eq!(row.gain_bits, gain.to_bits());
        assert_eq!(
            (row.failed_plans, row.last_pulse, row.pulse_valid),
            (0, 0, false)
        );
    }
    let new_gain = -0.0_f32;
    device
        .queue(
            GroupPacket::new(
                GroupHeader {
                    epoch: 1,
                    sequence: 2,
                    graph: 1,
                    topology: 1,
                    deadline: None,
                },
                &[CommandRecord::gain(recipe.target(ids[0]), new_gain)],
            )
            .unwrap(),
        )
        .unwrap();
    let mut published = false;
    for _ in 0..1000 {
        let before = ops.get();
        let step = device.publication_step().unwrap();
        assert!(ops.get() - before <= MAX_PUBLICATION_BUS_OPS);
        if matches!(step, PublicationStep::Published { .. }) {
            published = true;
            break;
        }
    }
    assert!(published);
    sim.borrow_mut().idle(100_000);
    let mut buffered = false;
    for _ in 0..120 {
        if device.result_step().unwrap() == ResultStep::Buffered {
            buffered = true;
            break;
        }
    }
    assert!(buffered);
    let result = device.take_result().unwrap();
    assert_eq!(result.codes(), [Code::Applied]);
    let attempt = result.header().frame;
    let mut seen = false;
    for _ in 0..3 {
        sim.borrow_mut().idle(80_000);
        for _ in 0..11_000 {
            if matches!(device.snapshot_step().unwrap(), SnapshotStep::Updated(_)) {
                break;
            }
        }
        let snapshot = device.latest_snapshot().unwrap();
        if snapshot.header().frame >= attempt {
            assert_eq!(
                snapshot.physical_row(0).unwrap().gain_bits,
                new_gain.to_bits()
            );
            assert_eq!(
                snapshot.physical_row(1).unwrap().gain_bits,
                gains[1].to_bits()
            );
            seen = true;
            break;
        }
    }
    assert!(seen);
    assert_eq!(Axi(&sim).read(REG_AUDIO_XRUNS), Ok(0));
    assert_eq!(Axi(&sim).read(REG_STATUS).unwrap() & 4, 0);
}
