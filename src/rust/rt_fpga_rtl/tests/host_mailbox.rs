#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]

use rt_fpga_rtl::{
    bits::Bits,
    component::{Instruction, instruction_words},
    host::{Client, Contract, Error, Mailbox, RegisterBus, Request, registers::*},
    model::Design,
    pipeline::{Geometry, design},
    sim::Sim,
    target::{ConfigurationWrite, Space},
};
use shoop_wasm_test_support::shoop_test;
use std::cell::RefCell;

struct Axi<'a, 'model>(&'a RefCell<Sim<'model>>);

impl Axi<'_, '_> {
    fn write_word(&self, offset: u32, value: u32, strobes: u64) -> Result<(), u64> {
        let mut sim = self.0.borrow_mut();
        sim.model.set_all(&[
            ("s_axi_awaddr", u64::from(offset)),
            ("s_axi_wdata", u64::from(value)),
            ("s_axi_wstrb", strobes),
        ]);
        let mut channels = [
            ("s_axi_awvalid", "s_axi_awready"),
            ("s_axi_wvalid", "s_axi_wready"),
        ];
        if offset & 4 != 0 {
            channels.reverse();
        }
        for (valid, ready) in channels {
            sim.model.set(valid, 1);
            assert_eq!(sim.tick(&[ready]).0.get(ready), 1);
            sim.model.set(valid, 0);
        }
        sim.idle(1);
        assert_eq!(sim.model.get("s_axi_bvalid"), 1);
        assert_eq!(sim.model.get("s_axi_awready"), 0);
        assert_eq!(sim.model.get("s_axi_wready"), 0);
        let code = sim.model.get("s_axi_bresp");
        sim.idle(2);
        assert_eq!(sim.model.get("s_axi_bresp"), code);
        assert_eq!(sim.model.get("s_axi_bvalid"), 1);
        sim.model.set("s_axi_bready", 1);
        sim.idle(1);
        sim.model.set("s_axi_bready", 0);
        if code == 0 { Ok(()) } else { Err(code) }
    }
}

impl RegisterBus for Axi<'_, '_> {
    type Error = u64;

    fn read(&mut self, offset: u32) -> Result<u32, u64> {
        let mut sim = self.0.borrow_mut();
        sim.model
            .set_all(&[("s_axi_arvalid", 1), ("s_axi_araddr", u64::from(offset))]);
        assert_eq!(sim.tick(&["s_axi_arready"]).0.get("s_axi_arready"), 1);
        sim.model.set("s_axi_arvalid", 0);
        let data = sim.model.get("s_axi_rdata") as u32;
        let code = sim.model.get("s_axi_rresp");
        sim.idle(2);
        assert_eq!(sim.model.get("s_axi_rdata"), u64::from(data));
        assert_eq!(sim.model.get("s_axi_rvalid"), 1);
        sim.model.set("s_axi_rready", 1);
        sim.idle(1);
        sim.model.set("s_axi_rready", 0);
        if code == 0 { Ok(data) } else { Err(code) }
    }

    fn write(&mut self, offset: u32, value: u32) -> Result<(), u64> {
        self.write_word(offset, value, 0xf)
    }
}

fn reply(sim: &RefCell<Sim<'_>>, epoch: u64, tag: u64, data: [u32; HOST_PAYLOAD_WORDS]) {
    let mut sim = sim.borrow_mut();
    sim.model.set_all(&[
        ("response_valid", 1),
        ("response_epoch", epoch),
        ("response_tag", tag),
        ("response_code", 42),
    ]);
    sim.model.set_bits("response_data", &Bits::from(data));
    sim.idle(1);
    sim.model.set("response_valid", 0);
}

#[shoop_test(no_wasm = "AXI register transaction test uses native Marlin")]
fn mailbox_preserves_requests_and_latched_responses_across_bus_stalls() {
    let root = rt_fpga_rtl::root();
    let rtl = root.join("fpga/host_bridge/rtl");
    let compiled = Design::new(
        "host_mailbox",
        vec![rtl.join("host_registers.sv"), rtl.join("host_mailbox.sv")],
    )
    .build(&root.join("work/host-mailbox"))
    .unwrap();
    let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
    sim.model.set_all(&[("reset", 1), ("epoch", 7)]);
    sim.idle(2);
    sim.model.set("reset", 0);
    sim.idle(2);
    let sim = RefCell::new(sim);
    let contract = Contract {
        clock_hz: 100_000_000,
        arena_base: 0x30000000,
        arena_bytes: 0x08000000,
    };
    assert!(matches!(
        Mailbox::open(
            Axi(&sim),
            Contract {
                clock_hz: 0,
                ..contract
            }
        ),
        Err(Error::Incompatible)
    ));
    let mut device = Mailbox::open(Axi(&sim), contract).unwrap();
    assert_eq!(Axi(&sim).write_word(REG_REQUEST_OP, 3, 0x7), Err(2));
    assert_eq!(Axi(&sim).read(REG_RESPONSE_DATA + 1), Err(2));
    let request = [0xdeadbeef, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    device.submit(11, 13, &request).unwrap();
    assert_eq!(device.submit(12, 99, &[]), Err(Error::Full));
    assert_eq!(Axi(&sim).write(REG_REQUEST_PUSH, 99), Err(2));
    {
        let mut core = sim.borrow_mut();
        assert_eq!(core.model.get("command_valid"), 1);
        assert_eq!(core.model.get("command_tag"), 11);
        assert_eq!(core.model.get("command_op"), 13);
        assert_eq!(
            core.model
                .bits("command_data")
                .array::<HOST_PAYLOAD_WORDS>(),
            request
        );
        core.model.set("command_ready", 1);
        core.idle(1);
        core.model.set("command_ready", 0);
    }
    reply(&sim, 6, 11, [99; HOST_PAYLOAD_WORDS]);
    assert_eq!(device.response(), Ok(None));
    reply(&sim, 7, 11, request);
    assert_eq!(sim.borrow().model.get("irq"), 1);
    assert_eq!(sim.borrow().model.get("response_ready"), 0);
    reply(&sim, 7, 99, [99; HOST_PAYLOAD_WORDS]);
    assert_eq!(Axi(&sim).write(REG_RESPONSE_POP, 99), Err(2));
    let response = device.response().unwrap().unwrap();
    assert_eq!(
        (response.tag, response.code, response.data),
        (11, 42, request)
    );
    assert_eq!(sim.borrow().model.get("irq"), 0);

    device.submit(12, 19, &[20]).unwrap();
    {
        let mut core = sim.borrow_mut();
        let data = core
            .model
            .bits("command_data")
            .array::<HOST_PAYLOAD_WORDS>();
        assert_eq!(data[0], 20);
        assert!(data[1..].iter().all(|&word| word == 0));
        core.model.set("command_ready", 1);
        core.idle(1);
        core.model.set("command_ready", 0);
    }
    reply(&sim, 7, 12, [20; HOST_PAYLOAD_WORDS]);
    sim.borrow_mut().model.set("fault", 1);
    assert_eq!(device.submit(13, 20, &[]), Err(Error::Fault));
    assert_eq!(device.response().unwrap().unwrap().tag, 12);
    assert_eq!(device.response(), Err(Error::Fault));

    sim.borrow_mut()
        .model
        .set_all(&[("epoch", 8), ("fault", 0)]);
    sim.borrow_mut().idle(2);
    assert_eq!(device.response(), Err(Error::EpochChanged));
    let mut axil = device.into_bus();
    assert_eq!(axil.write(REG_REQUEST_EPOCH, 7), Ok(()));
    assert_eq!(axil.write(REG_REQUEST_PUSH, 14), Err(2));
    let mut device = Mailbox::open(axil, contract).unwrap();
    assert_eq!(device.response(), Ok(None));
    device.submit(14, 20, &[]).unwrap();
}

#[shoop_test(no_wasm = "FPGA pipeline with the real AXI register wrapper")]
fn run_compiled_programs_and_switch_at_the_same_boundary_through_axi() {
    let geometry = Geometry::new([4, 32, 1], 2);
    let limits = rt_fpga_compiler::ProgramLimits {
        instructions: 16,
        state_values: 8,
        loop_slots: 4,
        jobs: 8,
        buffers: 4,
        history_slots: 4,
        input_ports: 1,
        output_ports: 2,
        quantum: 4,
    };
    let mut design = design(&geometry, limits);
    design.top = "host_pipeline_axi".into();
    design.probes.clear();
    design.parameters.remove("PHYSICAL_BASE");
    design = design.parameters(&[("ARENA_BASE", 0x01000000), ("QUANTUM", 4)]);
    let root = rt_fpga_rtl::root();
    for source in [
        "host_registers.sv",
        "host_mailbox.sv",
        "host_dispatch.sv",
        "host_pipeline_axi.sv",
    ] {
        design
            .sources
            .push(root.join("fpga/host_bridge/rtl").join(source));
    }
    let compiled = design.build(&root.join("work/host-mailbox")).unwrap();
    let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
    sim.model
        .set_all(&[("reset", 1), ("epoch", 1), ("sync_event_ready", 1)]);
    sim.idle(2);
    sim.model.set("reset", 0);
    sim.idle(2);
    let sim = RefCell::new(sim);
    let mut device = Mailbox::open(
        Axi(&sim),
        Contract {
            clock_hz: 100_000_000,
            arena_base: 0x01000000,
            arena_bytes: 0x08000000,
        },
    )
    .unwrap();
    let mut tag = 0;
    let mut send = |op, args: &[u32], expected| {
        tag += 1;
        device.submit(tag, op, args).unwrap();
        let response = (0..1000)
            .find_map(|_| device.response().unwrap())
            .expect("pipeline command timeout");
        assert_eq!(
            (response.tag, response.code),
            (tag, expected),
            "operation {op}"
        );
        response.data
    };
    send(0, &[], RESULT_INVALID);
    for view in 0..geometry.views as u32 {
        send(OP_STORE_CONFIG, &[0x101, view, 1], RESULT_OK);
    }
    for (address, value) in [
        (0, geometry.views as u32),
        (1, 0),
        (2, 0),
        (3, geometry.blocks() as u32),
    ] {
        send(OP_STORE_CONFIG, &[0x100, address, value], RESULT_OK);
    }
    let plan = [
        Instruction::ProcessInput {
            port: 0,
            destination: 0,
        },
        Instruction::ProcessOutput { source: 0, port: 0 },
        Instruction::ProcessOutput { source: 0, port: 1 },
    ];
    for region in 0..2 {
        for (pc, instruction) in plan.into_iter().enumerate() {
            let words = instruction_words(instruction).unwrap();
            let mut args = [0; 7];
            args[0] = 0x100;
            args[1] = region * limits.instructions + pc as u32;
            args[2..].copy_from_slice(&words);
            send(OP_CORE_CONFIG, &args, RESULT_OK);
        }
    }
    let first = send(
        OP_SWITCH_PROGRAM,
        &[0, 0, 1, 0, 0, 3, 0, 1, 3, 1],
        RESULT_OK,
    );
    assert_eq!(&first[..4], &[0, 0, 1, 0]);
    let samples = [0x00010000, 0x01000000, 0xff000000, 0x12345678];
    for (i, sample) in samples.into_iter().enumerate() {
        send(OP_AUDIO_CONFIG, &[0x100, i as u32, sample], RESULT_OK);
    }
    let result = send(OP_RUN_QUANTUM, &[], RESULT_OK);
    assert!(result[0] > 0);
    assert_eq!(&result[1..5], &[4, 0, 1, 0]);
    for port in 0..2 {
        for (i, expected) in samples.into_iter().enumerate() {
            let result = send(
                OP_AUDIO_CONFIG,
                &[1, port * limits.quantum + i as u32],
                RESULT_OK,
            );
            assert_eq!(result[0], expected);
        }
    }
    send(
        OP_SWITCH_PROGRAM,
        &[0, 0, 2, 0, 1, 3, 0, 1, 3, 1],
        RESULT_REJECTED,
    );
    let switched = send(
        OP_SWITCH_PROGRAM,
        &[1, 0, 2, 0, 1, 3, 0, 1, 3, 1],
        RESULT_OK,
    );
    assert_eq!(&switched[..4], &[4, 0, 2, 0]);
    let result = send(OP_RUN_QUANTUM, &[], RESULT_OK);
    assert_eq!(&result[1..5], &[8, 0, 2, 0]);

    send(
        OP_CORE_CONFIG,
        &[0x102, 2, 0x789abcde, 0x12345678],
        RESULT_OK,
    );
    let snapshot = send(OP_CAPTURE_SNAPSHOT, &[1], RESULT_OK);
    assert_eq!(&snapshot[..6], &[1, 8, 0, 2, 0, 1]);
    send(OP_CORE_CONFIG, &[0x102, 2, 7], RESULT_OK);
    let saved = send(OP_READ_SNAPSHOT, &[snapshot[0], 2, 1], RESULT_OK);
    assert_eq!(&saved[..2], &[0x789abcde, 0x12345678]);
    let next = send(OP_CAPTURE_SNAPSHOT, &[1], RESULT_OK);
    assert_eq!(&next[..6], &[2, 8, 0, 2, 0, 1]);
    send(OP_READ_SNAPSHOT, &[snapshot[0], 2, 1], RESULT_INVALID);
    let saved = send(OP_READ_SNAPSHOT, &[next[0], 0, 6], RESULT_OK);
    assert_eq!(&saved[4..6], &[7, 0]);

    let mut core = sim.borrow_mut();
    core.model.set("quantum_valid", 1);
    core.until(32, "accept hardware-paced quantum", |model| {
        model.get("quantum_ready") != 0
    });
    core.idle(1);
    core.model.set("quantum_valid", 0);
    core.until(1000, "complete hardware-paced quantum", |model| {
        model.get("quantum_done") != 0
    });
    assert_eq!(core.model.get("quantum_frame"), 12);
    assert_eq!(core.model.get("quantum_revision"), 2);
    assert!(core.model.get("quantum_cycles") > 0);
    drop(core);

    let mut client = Client::new(device);
    let value = 0x123456789abcdef0;
    let write = ConfigurationWrite {
        space: Space::Loop,
        address: 2,
        value: Bits::from_u64(value),
    };
    let mut execute = |request: Request| {
        client.start(request).unwrap();
        assert_eq!(client.start(Request::run_quantum()), Err(Error::Full));
        let result = (0..1000)
            .find_map(|_| client.poll().unwrap())
            .expect("typed operation timeout");
        assert_eq!(result.code, RESULT_OK);
        result.data
    };
    execute(Request::configuration_write(&write).unwrap());
    let state = execute(Request::capture_snapshot(1));
    assert_eq!(&state[..6], &[3, 12, 0, 2, 0, 1]);
    let word = execute(Request::read_snapshot(state[0], 2, 1));
    assert_eq!(&word[..2], &[value as u32, (value >> 32) as u32]);
    execute(Request::input_sample(0, 0x32100000));
    let result = execute(Request::run_quantum());
    assert_eq!(&result[1..5], &[16, 0, 2, 0]);
    assert_eq!(execute(Request::output_sample(0))[0], 0x32100000);
}
