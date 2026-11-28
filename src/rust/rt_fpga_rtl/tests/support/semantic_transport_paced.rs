use super::*;
#[path = "semantic_transport_owned.rs"]
mod owned;
#[path = "semantic_transport_preparation.rs"]
mod preparation;
use rt_fpga_rtl::{
    component::{Instruction, instruction_words},
    host::semantic::{
        CommandRecord, GroupHeader, GroupPacket, MAX_RESULT_BUS_OPS, MAX_SNAPSHOT_BUS_OPS,
        ResultStep, SNAPSHOT_COPY_WORDS, SnapshotBuffers, SnapshotPhase, SnapshotStep,
        SwitchOutcome,
    },
};
use shoop_rt_protocol::immediate::{Code, SwitchProgram};

#[derive(Clone)]
struct Observed<B> {
    bus: B,
    words: Rc<Cell<usize>>,
    acknowledgements: Rc<Cell<usize>>,
    lose_ack: Rc<Cell<bool>>,
    snapshot_words: Rc<Cell<usize>>,
    snapshot_acks: Rc<Cell<usize>>,
    lose_snapshot_at: Rc<Cell<usize>>,
}
impl<B: RegisterBus<Error = u64>> RegisterBus for Observed<B> {
    type Error = u64;
    fn read(&mut self, address: u32) -> Result<u32, u64> {
        let value = self.bus.read(address)?;
        if address == REG_RESULT_DATA {
            self.words.set(self.words.get() + 1);
        }
        if address == REG_SNAPSHOT_DATA {
            self.snapshot_words.set(self.snapshot_words.get() + 1);
        }
        Ok(value)
    }
    fn write(&mut self, address: u32, value: u32) -> Result<(), u64> {
        self.bus.write(address, value)?;
        if address == REG_RESULT_ACK {
            self.acknowledgements.set(self.acknowledgements.get() + 1);
            if self.lose_ack.replace(false) {
                return Err(99);
            }
        }
        if address == REG_SNAPSHOT_ACK {
            self.snapshot_acks.set(self.snapshot_acks.get() + 1);
            if self.lose_snapshot_at.get() == self.snapshot_acks.get() {
                self.lose_snapshot_at.set(0);
                return Err(99);
            }
        }
        Ok(())
    }
}
fn paced_design() -> Design {
    let root = rt_fpga_rtl::root();
    let mut design = design();
    design.top = "fifo_audio_host_axi".into();
    design.probes = ["last_admitted", "head_sequence", "release_sequence"]
        .map(|name| {
            Probe::inject(&format!(
                "control.pipeline.execution.core.pipeline.consumer.banks.{name}"
            ))
        })
        .into();
    for module in [
        "fifo_audio_host_axi.sv",
        "host_generated_audio.v",
        "host_i2s_audio.v",
    ] {
        design
            .sources
            .push(root.join("fpga/host_bridge/rtl").join(module));
    }
    for module in ["audio_io.sv", "audio_lane_ram.sv", "i2s_duplex.sv"] {
        design
            .sources
            .push(root.join("fpga/audio_io/rtl").join(module));
    }
    design
}
fn service(sim: &RefCell<Sim<'_>>, operation: u32, payload: [u32; 12], tag: u32) {
    let mut bus = Axi(sim);
    assert_eq!(bus.read(REG_STATUS).unwrap() & 3, 1);
    bus.write(REG_REQUEST_EPOCH, 1).unwrap();
    bus.write(REG_REQUEST_OP, operation).unwrap();
    for (index, word) in payload.into_iter().enumerate() {
        bus.write(REG_REQUEST_DATA + 4 * index as u32, word)
            .unwrap();
    }
    bus.write(REG_REQUEST_PUSH, tag).unwrap();
    assert_eq!(bus.read(REG_SUBMISSION_RESULT), Ok(0));
    let mut completed = false;
    for _ in 0..20_000 {
        if bus.read(REG_STATUS).unwrap() & 2 != 0 {
            completed = true;
            break;
        }
    }
    assert!(completed);
    assert_eq!(bus.read(REG_RESPONSE_TAG), Ok(tag));
    assert_eq!(bus.read(REG_RESPONSE_CODE), Ok(0));
    bus.write(REG_RESPONSE_POP, tag).unwrap();
    assert_eq!(bus.read(REG_SUBMISSION_RESULT), Ok(0));
}
fn publish<B: RegisterBus<Error = u64>>(device: &mut Device<B>, ops: &Cell<usize>) -> usize {
    for _ in 0..1000 {
        let before = ops.get();
        let step = device.publication_step().unwrap();
        assert!(ops.get() - before <= MAX_PUBLICATION_BUS_OPS);
        if let PublicationStep::Published { bank, .. } = step {
            return bank;
        }
    }
    panic!("publication did not complete");
}
fn buffer<B: RegisterBus<Error = u64>>(device: &mut Device<B>, ops: &Cell<usize>) {
    for _ in 0..120 {
        let before = ops.get();
        let step = device.result_step().unwrap();
        assert!(ops.get() - before <= MAX_RESULT_BUS_OPS);
        if step == ResultStep::Buffered {
            return;
        }
        assert!(device.take_result().is_none());
    }
    panic!("copy did not complete");
}
fn group(sequence: u64, graph: u64, count: usize, deadline: Option<u64>) -> GroupPacket {
    let records = [CommandRecord::gain(None, f32::NAN); 32];
    GroupPacket::new(
        GroupHeader {
            epoch: 1,
            sequence,
            graph,
            topology: graph,
            deadline,
        },
        &records[..count],
    )
    .unwrap()
}

#[shoop_test(
    no_wasm = "Real full-profile audio pacing, semantic result MMIO and native fence/recovery"
)]
fn hardware_paced_results_survive_partial_read_stalls_bank_reuse_and_a_lost_real_acknowledgement() {
    let root = rt_fpga_rtl::root();
    let compiled = paced_design()
        .build(&root.join("work/cmd-fifo-results-paced"))
        .unwrap();
    for lost in [false, true] {
        let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
        sim.max_ps = 1_000_000_000_000;
        sim.model.set_all(&[("reset", 1), ("epoch", 1)]);
        sim.idle(40);
        sim.model.set("reset", 0);
        sim.idle(6000);
        let sim = RefCell::new(sim);
        let ops = Rc::new(Cell::new(0));
        let words = Rc::new(Cell::new(0));
        let acknowledgements = Rc::new(Cell::new(0));
        let snapshot_words = Rc::new(Cell::new(0));
        let snapshot_acks = Rc::new(Cell::new(0));
        let lose_snapshot_at = Rc::new(Cell::new(0));
        let resets = Rc::new(Cell::new(0));
        let bus = Observed {
            bus: Count {
                bus: Axi(&sim),
                ops: ops.clone(),
                resets: resets.clone(),
                lose_recovery: Rc::new(Cell::new(false)),
            },
            words: words.clone(),
            acknowledgements: acknowledgements.clone(),
            lose_ack: Rc::new(Cell::new(lost)),
            snapshot_words: snapshot_words.clone(),
            snapshot_acks: snapshot_acks.clone(),
            lose_snapshot_at: lose_snapshot_at.clone(),
        };
        let mut device = Device::open(bus, contract()).unwrap();
        for (pc, instruction) in [
            Instruction::ProcessInput {
                port: 0,
                destination: 0,
            },
            Instruction::ProcessOutput { source: 0, port: 0 },
            Instruction::ProcessOutput { source: 0, port: 1 },
        ]
        .into_iter()
        .enumerate()
        {
            let mut payload = [0; 12];
            payload[0] = 0x100;
            payload[1] = pc as u32;
            payload[2..7].copy_from_slice(&instruction_words(instruction).unwrap());
            service(&sim, SEMANTIC_SERVICE_CORE_CONFIG, payload, 100 + pc as u32);
        }
        let installation = GroupPacket::new(
            GroupHeader {
                epoch: 1,
                sequence: 1,
                graph: 0,
                topology: 0,
                deadline: None,
            },
            &[CommandRecord::switch(
                SwitchProgram {
                    revision: 1,
                    region: 0,
                    instructions: 3,
                    sync_count: 0,
                    states: 0,
                    buffers: 1,
                },
                false,
            )],
        )
        .unwrap();
        device.queue(installation).unwrap();
        assert_eq!(publish(&mut device, &ops), 0);
        sim.borrow_mut().idle(80_000);
        assert_eq!(sim.borrow().model.get("active_revision"), 0);
        assert_eq!(sim.borrow().model.get("sync_frame"), 0);
        assert_eq!(Axi(&sim).read(REG_RESULT_READY), Ok(0));
        let mut payload = [0; 12];
        payload[0] = 1;
        service(&sim, SEMANTIC_SERVICE_ENABLE_AUDIO, payload, 200);
        sim.borrow_mut().idle(100_000);
        assert_eq!(sim.borrow().model.get("active_revision"), 1);
        assert_eq!(Axi(&sim).read(REG_RESULT_READY), Ok(1));
        while words.get() < 3 {
            let before = ops.get();
            device.result_step().unwrap();
            assert!(ops.get() - before <= MAX_RESULT_BUS_OPS);
        }
        let frame = sim.borrow().model.get("sync_frame");
        let quanta = Axi(&sim).read(REG_COMPLETED_QUANTA).unwrap();
        sim.borrow_mut().idle(300_000);
        assert!(sim.borrow().model.get("sync_frame") >= frame + 128);
        assert!(Axi(&sim).read(REG_COMPLETED_QUANTA).unwrap() >= quanta + 4);
        assert_eq!(Axi(&sim).read(REG_SNAPSHOT_READY), Ok(3));
        assert!(Axi(&sim).read(REG_SNAPSHOT_SKIPPED_LOW).unwrap() > 0);
        assert_eq!(Axi(&sim).read(REG_AUDIO_XRUNS), Ok(0));
        assert_eq!(acknowledgements.get(), 0);
        assert!(device.take_result().is_none());
        if lost {
            let mut failed = false;
            for _ in 0..120 {
                let before = ops.get();
                match device.result_step() {
                    Err(Error::Bus(99)) => {
                        failed = true;
                        break;
                    }
                    Ok(ResultStep::Progress) => {}
                    other => panic!("unexpected ACK result {other:?}"),
                }
                assert!(ops.get() - before <= MAX_RESULT_BUS_OPS);
            }
            assert!(failed);
            assert_eq!(acknowledgements.get(), 1);
            assert_eq!(Axi(&sim).read(REG_FREE_BANKS), Ok(15));
            assert!(device.published(0).is_some());
            assert!(device.take_result().is_none());
            let before = ops.get();
            assert_eq!(device.result_step(), Err(Error::Fault));
            assert_eq!(device.publication_step(), Err(Error::Fault));
            assert_eq!(ops.get(), before);
            let mut recovery = device.recovery();
            recovery.begin().unwrap();
            let mut complete = false;
            for _ in 0..2000 {
                match recovery.poll().unwrap() {
                    RecoveryState::Ready(2) => {
                        complete = true;
                        break;
                    }
                    RecoveryState::Draining | RecoveryState::Resetting => {}
                    state => panic!("unexpected recovery {state:?}"),
                }
            }
            assert!(complete);
            let fresh = recovery.reopen().unwrap();
            assert_eq!(fresh.epoch(), 2);
            assert_eq!(fresh.next_sequence(), 1);
            assert_eq!(resets.get(), 1);
            assert_eq!(acknowledgements.get(), 1);
            assert_eq!(Axi(&sim).read(REG_COMPLETED_QUANTA), Ok(0));
            assert_eq!(sim.borrow().model.get("audio_enable"), 0);
        } else {
            buffer(&mut device, &ops);
            assert!(device.published(0).is_none());
            device.queue(group(2, 0, 16, None)).unwrap();
            assert_eq!(publish(&mut device, &ops), 0);
            sim.borrow_mut().idle(150_000);
            let before = ops.get();
            assert_eq!(device.result_step().unwrap(), ResultStep::Buffered);
            assert_eq!(ops.get(), before);
            let first = device.take_result().unwrap();
            let header = first.header();
            assert_eq!(header.identity.sequence(), 1);
            assert_eq!(header.frame, 0);
            assert_eq!(header.initial_revision, 0);
            assert_eq!(header.current_revision, 1);
            assert_eq!(header.first_frame, 0);
            assert_eq!(header.switch, SwitchOutcome::Fenced);
            assert_eq!(first.codes(), &[Code::Applied]);
            for (sequence, count, expected) in [
                (2, 16, Code::StaleConfiguration),
                (3, 32, Code::InvalidValue),
                (4, 2, Code::DeadlineMissed),
                (5, 1, Code::InvalidValue),
            ] {
                if sequence != 2 {
                    device
                        .queue(group(sequence, 1, count, (sequence == 4).then_some(0)))
                        .unwrap();
                    assert_eq!(publish(&mut device, &ops), 0);
                    sim.borrow_mut().idle(150_000);
                }
                buffer(&mut device, &ops);
                let result = device.take_result().unwrap();
                let header = result.header();
                assert_eq!(header.identity.sequence(), sequence);
                assert!(header.frame > 0);
                assert_eq!(header.initial_revision, 1);
                assert_eq!(header.current_revision, 1);
                assert_eq!(header.switch, SwitchOutcome::None);
                assert_eq!(header.first_frame, 0);
                assert_eq!(
                    header.code,
                    if matches!(sequence, 2 | 4) {
                        expected
                    } else {
                        Code::Applied
                    }
                );
                assert_eq!(result.codes(), vec![expected; count]);
                assert!(device.published(0).is_none());
            }
            assert_eq!(acknowledgements.get(), 5);
            assert_eq!(resets.get(), 0);
            assert_eq!(Axi(&sim).read(REG_AUDIO_XRUNS), Ok(0));
            let mut first = vec![0xa5a5_a5a5; SNAPSHOT_COPY_WORDS];
            let mut second = first.clone();
            let mut device =
                device.with_snapshots(SnapshotBuffers::new(&mut first, &mut second).unwrap());
            device.queue(group(6, 1, 1, None)).unwrap();
            let mut published = false;
            let mut copied_result = false;
            let mut paused = false;
            let mut snapshot = None;
            for _ in 0..12_000 {
                let before = ops.get();
                let step = device.snapshot_step().unwrap();
                assert!(ops.get() - before <= MAX_SNAPSHOT_BUS_OPS);
                if let SnapshotStep::Updated(header) = step {
                    snapshot = Some(header);
                    break;
                }
                assert!(device.latest_snapshot().is_none());
                if snapshot_words.get() == 35 && !paused {
                    let frame = sim.borrow().model.get("sync_frame");
                    sim.borrow_mut().idle(300_000);
                    assert!(sim.borrow().model.get("sync_frame") >= frame + 128);
                    paused = true;
                }
                let before = ops.get();
                if !published {
                    published = matches!(
                        device.publication_step().unwrap(),
                        PublicationStep::Published { .. }
                    );
                }
                assert!(ops.get() - before <= MAX_PUBLICATION_BUS_OPS);
                let before = ops.get();
                if !copied_result {
                    copied_result = device.result_step().unwrap() == ResultStep::Buffered;
                }
                assert!(ops.get() - before <= MAX_RESULT_BUS_OPS);
            }
            assert!(published && copied_result && paused);
            let snapshot = snapshot.expect("complete snapshot");
            assert_eq!(snapshot.sequence, 2);
            assert_eq!(snapshot.frame, 0);
            assert_eq!(snapshot.revision, 1);
            assert_eq!(snapshot.phase, SnapshotPhase::Settled);
            assert_eq!(device.latest_snapshot().unwrap().header(), snapshot);
            assert_eq!(
                device.take_result().unwrap().header().identity.sequence(),
                6
            );
            assert_eq!(
                snapshot_acks.get(),
                2,
                "older initial discarded, selected settled released"
            );
            assert_eq!(acknowledgements.get(), 6);
            sim.borrow_mut().idle(100_000);
            assert_eq!(Axi(&sim).read(REG_SNAPSHOT_READY), Ok(3));
            lose_snapshot_at.set(4);
            let mut failed = false;
            for _ in 0..12_000 {
                let before = ops.get();
                let result = device.snapshot_step();
                assert!(ops.get() - before <= MAX_SNAPSHOT_BUS_OPS);
                assert_eq!(device.latest_snapshot().unwrap().header(), snapshot);
                match result {
                    Err(Error::Bus(99)) => {
                        failed = true;
                        break;
                    }
                    Ok(SnapshotStep::Progress) => {}
                    other => panic!("unexpected snapshot ACK outcome {other:?}"),
                }
            }
            assert!(failed);
            assert_eq!(snapshot_acks.get(), 4);
            let before = ops.get();
            assert_eq!(device.snapshot_step(), Err(Error::Fault));
            assert_eq!(device.publication_step(), Err(Error::Fault));
            assert_eq!(device.result_step(), Err(Error::Fault));
            assert_eq!(ops.get(), before);
            assert_eq!(Axi(&sim).read(REG_AUDIO_XRUNS), Ok(0));
            let mut recovery = device.recovery();
            recovery.begin().unwrap();
            let mut recovered = false;
            for _ in 0..2000 {
                match recovery.poll().unwrap() {
                    RecoveryState::Ready(2) => {
                        recovered = true;
                        break;
                    }
                    RecoveryState::Draining | RecoveryState::Resetting => {}
                    state => panic!("unexpected snapshot recovery {state:?}"),
                }
            }
            assert!(recovered);
            let fresh = recovery.reopen().unwrap();
            let mut fresh =
                fresh.with_snapshots(SnapshotBuffers::new(&mut first, &mut second).unwrap());
            assert!(fresh.latest_snapshot().is_none());
            sim.borrow_mut().idle(6000);
            let mut initial = None;
            for _ in 0..12_000 {
                let before = ops.get();
                let result = fresh.snapshot_step().unwrap();
                assert!(ops.get() - before <= MAX_SNAPSHOT_BUS_OPS);
                if let SnapshotStep::Updated(header) = result {
                    initial = Some(header);
                    break;
                }
                assert!(fresh.latest_snapshot().is_none());
            }
            let initial = initial.expect("fresh initial copy");
            assert_eq!(initial.epoch, 2);
            assert_eq!(initial.sequence, 1);
            assert_eq!(initial.frame, 0);
            assert_eq!(initial.revision, 0);
            assert_eq!(initial.phase, SnapshotPhase::Initial);
            assert_eq!(resets.get(), 1);
            assert_eq!(snapshot_acks.get(), 5);
            assert_eq!(acknowledgements.get(), 6);
            assert_eq!(Axi(&sim).read(REG_COMPLETED_QUANTA), Ok(0));
            assert_eq!(sim.borrow().model.get("audio_enable"), 0);
        }
    }
}
