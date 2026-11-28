use super::*;
use rt_fpga_compiler::{
    Identity, InitialValue, LogicalStateProgram, Operation, Ramp, Resources, StateBinding,
    StateKey, StateRole, loading::LoadError,
};
use rt_fpga_rtl::host::semantic::{MAX_PREPARATION_BUS_OPS, PreparationProgress, SelectionStep};

#[derive(Clone)]
struct LostPop<B> {
    bus: B,
    lose: Rc<Cell<bool>>,
}
impl<B: RegisterBus<Error = u64>> RegisterBus for LostPop<B> {
    type Error = u64;
    fn read(&mut self, address: u32) -> Result<u32, u64> {
        self.bus.read(address)
    }
    fn write(&mut self, address: u32, value: u32) -> Result<(), u64> {
        self.bus.write(address, value)?;
        if address == REG_RESPONSE_POP && self.lose.replace(false) {
            return Err(99);
        }
        Ok(())
    }
}

fn program(replacement: bool) -> LogicalStateProgram {
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
    let gains = if replacement {
        vec![0.75_f32, -0.0]
    } else {
        vec![0.12345679_f32]
    };
    let ids = &ids[..gains.len()];
    LogicalStateProgram {
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
            .map(|&id| rt_fpga_compiler::Loop {
                id,
                length: 0,
                sync_source: None,
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
    }
}

#[shoop_test(
    no_wasm = "Actual reset-owned ProgramSession and bounded configuration/publication/native fences"
)]
fn reset_bound_session_prepares_selects_and_replaces_without_pacing_real_audio() {
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
    let lose_pop = Rc::new(Cell::new(false));
    let device = Device::open(
        LostPop {
            bus: Count {
                bus: Axi(&sim),
                ops: ops.clone(),
                resets: Rc::new(Cell::new(0)),
                lose_recovery: Rc::new(Cell::new(false)),
            },
            lose: lose_pop.clone(),
        },
        contract(),
    )
    .unwrap();
    let device = match device.program_session() {
        Err(device) => device,
        Ok(_) => panic!("discovery must not authorize unused leases"),
    };
    let mut recovery = device.recovery();
    recovery.begin().unwrap();
    let mut reset = false;
    for _ in 0..10_000 {
        if recovery.poll().unwrap() == RecoveryState::Ready(2) {
            reset = true;
            break;
        }
    }
    assert!(reset);
    let (mut allocator, mut owner) = recovery
        .reopen()
        .unwrap()
        .program_session()
        .unwrap_or_else(|_| panic!("genuine reset did not bind an allocator"));
    assert_eq!(owner.epoch(), 2);
    let mut logical = program(false);
    let first = allocator.stage(&logical).unwrap();
    logical.bindings[0].initial_gain_bits = Some(f32::NAN.to_bits());
    owner.stage(first.clone()).unwrap();
    assert_eq!(
        owner.staged().unwrap().compiled.bindings[0].initial_gain_bits,
        Some(0.12345679_f32.to_bits())
    );
    assert_eq!(allocator.cancel(first.revision), Err(LoadError::Busy));
    let mut prepared = false;
    for _ in 0..20_000 {
        let before = ops.get();
        let step = owner.preparation_step().unwrap();
        assert!(ops.get() - before <= MAX_PREPARATION_BUS_OPS);
        if matches!(step, PreparationProgress::Prepared(_)) {
            prepared = true;
            break;
        }
    }
    assert!(prepared);
    assert_eq!(Axi(&sim).read(REG_COMPLETED_QUANTA), Ok(0));
    assert_eq!(sim.borrow().model.get("sync_frame"), 0);
    owner.select(None).unwrap();
    allocator.publish(first.revision).unwrap();
    assert_eq!(allocator.complete(first.revision), Err(LoadError::Busy));
    let mut published = false;
    for _ in 0..200 {
        let before = ops.get();
        let step = owner.selection_step().unwrap();
        assert!(ops.get() - before <= 3);
        if matches!(step, SelectionStep::Published(_)) {
            published = true;
            break;
        }
    }
    assert!(published);
    sim.borrow_mut().idle(20_000);
    assert_eq!(sim.borrow().model.get("active_revision"), 0);
    assert_eq!(Axi(&sim).read(REG_COMPLETED_QUANTA), Ok(0));
    let mut selected = false;
    for _ in 0..20_000 {
        let before = ops.get();
        let step = owner.selection_step().unwrap();
        assert!(ops.get() - before <= 3);
        if let SelectionStep::Fenced {
            selected: active,
            retired,
        } = step
        {
            assert_eq!((active.revision, active.frame), (first.revision, 0));
            assert!(retired.is_none());
            selected = true;
            break;
        }
        assert_eq!(allocator.complete(first.revision), Err(LoadError::Busy));
    }
    assert!(selected);
    allocator.complete(first.revision).unwrap();
    let gain = owner.active().unwrap().compiled.bindings[0].gain.unwrap();
    let next = allocator.stage(&program(true)).unwrap();
    owner.stage(next.clone()).unwrap();
    assert!(
        owner
            .staged()
            .unwrap()
            .initializers
            .iter()
            .all(|(address, _)| *address != gain)
    );
    assert_eq!(owner.staged().unwrap().layout.new_loops().count(), 1);
    // Three body words, no PUSH: the private preparation lane must not own audio progress.
    for _ in 0..11 {
        let before = ops.get();
        owner.preparation_step().unwrap();
        assert!(ops.get() - before <= 3);
    }
    let before = Axi(&sim).read(REG_COMPLETED_QUANTA).unwrap();
    sim.borrow_mut().idle(300_000);
    assert!(Axi(&sim).read(REG_COMPLETED_QUANTA).unwrap() >= before + 4);
    assert_eq!(sim.borrow().model.get("active_revision"), first.revision);
    assert_eq!(Axi(&sim).read(REG_AUDIO_XRUNS), Ok(0));
    prepared = false;
    for _ in 0..20_000 {
        let before = ops.get();
        let step = owner.preparation_step().unwrap();
        assert!(ops.get() - before <= 3);
        if matches!(step, PreparationProgress::Prepared(_)) {
            prepared = true;
            break;
        }
    }
    assert!(prepared);
    owner.select(None).unwrap();
    allocator.publish(next.revision).unwrap();
    selected = false;
    for _ in 0..20_000 {
        let before = ops.get();
        let step = owner.selection_step().unwrap();
        assert!(ops.get() - before <= 3);
        if let SelectionStep::Fenced {
            selected: active,
            retired,
        } = step
        {
            assert_eq!(active.revision, next.revision);
            assert!(active.frame > 0);
            assert_eq!(retired.unwrap().image().layout.revision, first.revision);
            selected = true;
            break;
        }
        assert_eq!(allocator.complete(next.revision), Err(LoadError::Busy));
    }
    assert!(selected);
    allocator.complete(next.revision).unwrap();
    assert_eq!(owner.active().unwrap().layout.revision, next.revision);
    assert_eq!(sim.borrow().model.get("active_revision"), next.revision);
    assert_eq!(Axi(&sim).read(REG_AUDIO_XRUNS), Ok(0));
    assert_eq!(Axi(&sim).read(REG_STATUS).unwrap() & 4, 0);
    let third = allocator.stage(&program(true)).unwrap();
    owner.stage(third.clone()).unwrap();
    lose_pop.set(true);
    let mut uncertain = false;
    for _ in 0..1000 {
        let before = ops.get();
        let step = owner.preparation_step();
        assert!(ops.get() - before <= 3);
        if step == Err(Error::Bus(99)) {
            uncertain = true;
            break;
        }
    }
    assert!(uncertain);
    assert!(!lose_pop.get());
    assert_eq!(Axi(&sim).read(REG_STATUS).unwrap() & 3, 1);
    assert_eq!(allocator.cancel(third.revision), Err(LoadError::Busy));
    let before = ops.get();
    assert_eq!(owner.preparation_step(), Err(Error::Fault));
    assert_eq!(ops.get(), before);
    let frames = Axi(&sim).read(REG_COMPLETED_QUANTA).unwrap();
    sim.borrow_mut().idle(300_000);
    assert!(Axi(&sim).read(REG_COMPLETED_QUANTA).unwrap() >= frames + 4);
    assert_eq!(owner.active().unwrap().layout.revision, next.revision);
    let mut recovery = owner.recovery();
    assert!(matches!(recovery.reopen(), Err(Error::Full)));
    recovery.begin().unwrap();
    reset = false;
    for _ in 0..10_000 {
        if recovery.poll().unwrap() == RecoveryState::Ready(3) {
            reset = true;
            break;
        }
    }
    assert!(reset);
    let (_, mut fresh) = recovery.reopen().unwrap();
    assert_eq!(fresh.epoch(), 3);
    assert!(fresh.active().is_none());
    assert_eq!(fresh.stage(third), Err(LoadError::IncompatibleState));
    assert_eq!(Axi(&sim).read(REG_COMPLETED_QUANTA), Ok(0));
    assert_eq!(Axi(&sim).read(REG_AUDIO_XRUNS), Ok(0));
}
