#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]

use rt_fpga_rtl::{
    bits::Bits,
    component::{Instruction, instruction_words},
    host::{Client, Contract, Error, Mailbox, RegisterBus, Request, registers::*},
    model::Design,
    pipeline::Geometry,
    sim::Sim,
    target::{ConfigurationWrite, Space},
};
use shoop_wasm_test_support::shoop_test;
use std::cell::RefCell;

#[path = "support/host_axi.rs"]
mod host_axi;
use host_axi::{Axi, build_pipeline, pipeline_contract};

#[cfg(feature = "rt-interface")]
#[path = "../../rt_fpga_compiler/tests/support/mono.rs"]
mod mono;

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

#[shoop_test(no_wasm = "The native audio AXI register test uses Marlin")]
fn native_audio_holds_banks_until_a_complete_matching_write() {
    let root = rt_fpga_rtl::root();
    let bridge = root.join("fpga/host_bridge");
    let rtl = bridge.join("rtl");
    let spec: serde_json::Value =
        serde_json::from_slice(&std::fs::read(bridge.join("native_audio.json")).unwrap()).unwrap();
    let reg = |name| {
        u32::from_str_radix(
            spec["registers"][name]
                .as_str()
                .unwrap()
                .trim_start_matches("0x"),
            16,
        )
        .unwrap()
    };
    let q = spec["quantum"].as_u64().unwrap() as u32;
    let compiled = Design::new(
        "native_audio_mmio",
        vec![
            rtl.join("native_audio_registers.sv"),
            rtl.join("native_audio_mmio.sv"),
        ],
    )
    .build(&root.join("work/host-mailbox"))
    .unwrap();
    let mut model = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
    model.model.set("reset", 1);
    model.idle(2);
    model.model.set("reset", 0);
    let sim = RefCell::new(model);
    let mut axi = Axi(&sim);
    assert_eq!(axi.read(reg("QUANTUM")), Ok(q));
    assert_eq!(axi.read(reg("SAMPLE_RATE")), Ok(48_000));
    assert_eq!(axi.read(reg("CLOCK_HZ")), Ok(100_000_000));
    assert_eq!(axi.read(reg("RX_BASE")), Ok(0));
    let start = || {
        let mut sim = sim.borrow_mut();
        assert_eq!(sim.model.get("quantum_ready"), 1);
        sim.model.set("quantum_valid", 1);
        sim.idle(1);
        sim.model.set("quantum_valid", 0);
        assert_eq!(sim.model.get("irq"), 1);
    };
    axi.write(reg("ENABLE"), 1).unwrap();
    start();
    assert_eq!(axi.read(reg("STATUS")), Ok(1));
    assert_eq!(axi.read(reg("SEQUENCE")), Ok(1));
    sim.borrow_mut().model.set("sample_input_data", 0xffed_cbaa);
    assert_eq!(axi.read(reg("RX_BASE") + 4 * (q - 1)), Ok(0xffed_cbaa));
    for word in 0..2 * q - 1 {
        axi.write(reg("TX_BASE") + 4 * word, 0x1234_5678 + word)
            .unwrap();
    }
    let last = reg("TX_BASE") + 4 * (2 * q - 1);
    axi.write_word(last, 0, 7).unwrap();
    axi.write(reg("COMPLETE"), 1).unwrap();
    assert_eq!(axi.read(reg("STATUS")), Ok(1));
    axi.write(last, 0x7654_3210).unwrap();
    axi.write(reg("COMPLETE"), 0).unwrap();
    assert_eq!(axi.read(reg("STATUS")), Ok(1));
    axi.write(reg("COMPLETE"), 1).unwrap();
    assert_eq!(axi.read(reg("STATUS")), Ok(0));
    let peak = axi.read(reg("PEAK_CYCLES")).unwrap();
    assert!(peak > 64);

    start();
    assert_eq!(axi.read(reg("SEQUENCE")), Ok(2));
    axi.write(reg("COMPLETE"), 2).unwrap();
    assert_eq!(axi.read(reg("STATUS")), Ok(1));
    {
        let mut sim = sim.borrow_mut();
        sim.model.set_all(&[("audio_fault", 1), ("audio_xruns", 3)]);
        sim.idle(1);
    }
    assert_eq!(axi.read(reg("STATUS")), Ok(2));
    assert_eq!(axi.read(reg("XRUNS")), Ok(3));
    assert_eq!(axi.read(reg("PEAK_CYCLES")), Ok(peak));
    assert_eq!(axi.read(reg("RX_BASE")), Ok(0));
    axi.write(reg("ENABLE"), 0).unwrap();
    axi.write(reg("ENABLE"), 1).unwrap();
    let sim = sim.borrow();
    assert_eq!(sim.model.get("irq"), 0);
    assert_eq!(sim.model.get("audio_enable"), 0);
    assert_eq!(sim.model.get("quantum_ready"), 0);
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

    let event = |index| [index, 0, 1, 0, 0, 7, 64 + index, 0];
    {
        let mut core = sim.borrow_mut();
        core.model.set("sync_event_valid", 1);
        for index in 0..=128 {
            assert_eq!(core.model.get("sync_event_ready"), 1);
            core.model
                .set("active_revision", 0x100000000 + u64::from(index / 16));
            core.model
                .set_bits("sync_event_data", &Bits::from(event(index)));
            core.idle(1);
        }
        core.model.set("sync_event_valid", 0);
        assert_eq!(core.model.get("irq"), 1);
        assert_eq!(core.model.get("event_overflow"), 1);
    }
    assert_eq!(Axi(&sim).read(REG_EVENT_COUNT), Ok(0x80000080));
    assert_eq!(Axi(&sim).write(REG_EVENT_POP, 2), Err(2));
    for index in 0..128 {
        let actual = device.sync_event().unwrap().unwrap();
        assert_eq!(actual.data, event(index));
        assert_eq!(actual.revision, 0x100000000 + u64::from(index / 16));
    }
    assert_eq!(device.sync_event(), Ok(None));
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
    assert_eq!(Axi(&sim).read(REG_EVENT_COUNT), Ok(0));
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
    let compiled = build_pipeline(&geometry, limits, 4096);
    let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
    sim.model.set_all(&[("reset", 1), ("epoch", 1)]);
    sim.idle(2);
    sim.model.set("reset", 0);
    sim.idle(2);
    let sim = RefCell::new(sim);
    let mut device = Mailbox::open(Axi(&sim), pipeline_contract()).unwrap();
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
    send(OP_ENABLE_AUDIO, &[1], RESULT_REJECTED);
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
    execute(Request::enable_audio(true));
    assert_eq!(sim.borrow().model.get("audio_enable"), 1);
    let boundary = execute(Request::begin_control(2));
    assert_eq!(&boundary[..4], &[16, 0, 2, 0]);
    sim.borrow_mut().model.set("quantum_valid", 1);
    sim.borrow_mut().idle(16);
    assert_eq!(sim.borrow().model.get("quantum_ready"), 0);
    sim.borrow_mut().model.set("quantum_valid", 0);
    execute(Request::end_control());
    execute(Request::enable_audio(false));
    assert_eq!(sim.borrow().model.get("audio_enable"), 0);
    client.start(Request::enable_audio(true)).unwrap();
    let result = (0..1000).find_map(|_| client.poll().unwrap()).unwrap();
    assert_eq!(result.code, RESULT_REJECTED);

    client.start(Request::begin_control(2)).unwrap();
    let result = (0..1000).find_map(|_| client.poll().unwrap()).unwrap();
    assert_eq!(result.code, RESULT_OK);
    sim.borrow_mut().idle(4096);
    assert_eq!(client.poll(), Err(Error::Fault));
    assert_eq!(sim.borrow().model.get("audio_enable"), 0);
}

#[cfg(feature = "rt-interface")]
#[shoop_test(no_wasm = "Uses Marlin and the actual FPGA AXI bridge")]
fn recorded_take_and_composite_clock_survive_graph_switches_over_the_hardware_mailbox() {
    use rt_fpga_compiler::{
        CompileContext, Identity, ProgramLimits, compile_composites, compile_with_chunk_frames,
        loading::Allocator,
        shoop_graph::{CompositeKind, CompositeSpan, CompositeView, LoopMode},
    };
    use rt_fpga_rtl::{
        host::{
            Upload,
            control::{ActiveProgram, ControlWindow, ProgramSession, gain_value},
        },
        target::RtlTarget,
    };
    use shoop_rt_protocol::immediate::{Code, SwitchProgram, TransitionMode, TransitionPlan};

    fn timeout() -> impl FnMut() -> bool {
        let mut remaining = 10000;
        move || {
            remaining -= 1;
            remaining > 0
        }
    }

    fn send(client: &mut Client<Axi<'_, '_>>, request: Request) -> [u32; HOST_PAYLOAD_WORDS] {
        let result = client
            .exchange(request, timeout())
            .expect("bridge response deadline");
        assert_eq!(result.code, RESULT_OK, "{result:?}");
        result.data
    }

    fn stage(session: &mut ProgramSession<Axi<'_, '_>>, upload: Upload) -> SwitchProgram {
        assert!(session.stage(upload).is_ok());
        (0..1000)
            .find_map(|_| session.upload_step(timeout()).unwrap())
            .expect("image upload deadline")
    }

    fn mode(
        session: &mut ProgramSession<Axi<'_, '_>>,
        identity: Identity,
        next_mode: TransitionMode,
    ) {
        let active = session.active().unwrap();
        let physical = active.layout.loop_id(identity).unwrap();
        let revision = active.layout.revision;
        let expected_frame = if next_mode == TransitionMode::Recording {
            0
        } else {
            3 * u64::from(active.compiled.quantum)
        };
        let mut window = ControlWindow::begin(session.client(), revision, timeout()).unwrap();
        assert_eq!(window.frame(), expected_frame);
        assert_eq!(
            window.apply_transition(
                physical,
                None,
                None,
                TransitionPlan {
                    now: Some(next_mode),
                    ..TransitionPlan::default()
                }
            ),
            Ok(Code::Applied)
        );
        window.release().unwrap();
    }

    let limits = ProgramLimits {
        instructions: 64,
        state_values: 16,
        loop_slots: 8,
        jobs: 64,
        buffers: 16,
        history_slots: 16,
        input_ports: 1,
        output_ports: 2,
        quantum: 32,
    };
    let geometry = Geometry::new([limits.state_values.into(), 256, 32], 32);
    let compiled = build_pipeline(&geometry, limits, 4096);
    let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
    sim.model.set_all(&[("reset", 1), ("epoch", 1)]);
    sim.idle(2);
    sim.model.set("reset", 0);
    sim.idle(2);
    let sim = RefCell::new(sim);
    let mut client = Client::new(Mailbox::open(Axi(&sim), pipeline_contract()).unwrap());
    for view in 0..geometry.views as u32 {
        send(&mut client, Request::store_word(1, view, 1));
    }
    for (address, value) in [
        (0, geometry.views as u32),
        (1, 0),
        (2, 0),
        (3, geometry.blocks() as u32),
    ] {
        send(&mut client, Request::store_word(0, address, value));
    }

    let identity = Identity {
        slot: 0,
        generation: 1,
    };
    let looping = mono::MonoLoop {
        id: identity,
        source: None,
        input: true,
        output: true,
        gain: 1.0,
        repeat_sync: false,
    };
    let context = CompileContext {
        quantum: limits.quantum,
        limits,
    };
    let first = compile_with_chunk_frames(&mono::session(&[looping]), context, 32).unwrap();
    let mut allocator = Allocator::new(limits);
    let layout = allocator.stage(&first).unwrap();
    let media = layout.media_views(&first);
    let upload = Upload::new(
        RtlTarget {
            capacities: limits,
            media_views: &media,
            state_base: 0,
        }
        .build_image(&first, layout)
        .unwrap(),
        limits.instructions,
    );
    let mut session = ProgramSession::new(client);
    let requested = stage(&mut session, upload);
    allocator.publish(1).unwrap();
    let (selected, retired) = session.activate(requested, timeout()).unwrap();
    assert_eq!(
        selected,
        ActiveProgram {
            frame: 0,
            revision: 1
        }
    );
    assert!(retired.is_none());
    allocator.complete(1).unwrap();
    mode(&mut session, identity, TransitionMode::Recording);
    let take: Vec<i32> = (0..3 * limits.quantum)
        .map(|i| ((i as i32 % 16) - 8) * 0x10000)
        .collect();
    for samples in take.chunks(limits.quantum as usize) {
        for (i, &sample) in samples.iter().enumerate() {
            send(session.client(), Request::input_sample(i as u32, sample));
        }
        send(session.client(), Request::run_quantum());
    }
    mode(&mut session, identity, TransitionMode::Stopped);

    let silent = mono::MonoLoop {
        id: Identity {
            slot: 1,
            generation: 1,
        },
        input: false,
        ..looping
    };
    let next = compile_with_chunk_frames(&mono::session(&[looping, silent]), context, 32).unwrap();
    let layout = allocator.stage(&next).unwrap();
    assert_eq!(
        layout.loop_id(identity).unwrap(),
        session.active().unwrap().layout.loop_id(identity).unwrap()
    );
    let media = layout.media_views(&next);
    let upload = Upload::new(
        RtlTarget {
            capacities: limits,
            media_views: &media,
            state_base: 0,
        }
        .build_image(&next, layout)
        .unwrap(),
        limits.instructions,
    );
    let requested = stage(&mut session, upload);
    assert_eq!(session.active().unwrap().layout.revision, 1);
    allocator.publish(2).unwrap();
    let (selected, retired) = session.activate(requested, timeout()).unwrap();
    assert_eq!(
        selected,
        ActiveProgram {
            frame: 3 * u64::from(limits.quantum),
            revision: 2
        }
    );
    assert_eq!(retired.unwrap().image.layout.revision, 1);
    allocator.complete(2).unwrap();
    mode(&mut session, identity, TransitionMode::Playing);
    let active = session.active().unwrap();
    let gain = active
        .compiled
        .bindings
        .iter()
        .find(|binding| binding.id == active.layout.loop_id(identity).unwrap())
        .unwrap()
        .gain
        .unwrap();
    for (index, reference) in take.chunks(limits.quantum as usize).enumerate() {
        if index == 1 {
            send(session.client(), Request::begin_control(2));
            let write = ConfigurationWrite {
                space: Space::State,
                address: u64::from(gain),
                value: gain_value(0.5),
            };
            send(
                session.client(),
                Request::configuration_write(&write).unwrap(),
            );
            send(session.client(), Request::end_control());
        }
        send(session.client(), Request::run_quantum());
        for port in 0..2 {
            for (i, &sample) in reference.iter().enumerate() {
                let actual = send(
                    session.client(),
                    Request::output_sample(port * limits.quantum + i as u32),
                );
                let expected = if index == 0 { sample } else { sample / 2 };
                assert_eq!(actual[0] as i32, expected, "port {port}, sample {i}");
            }
        }
    }

    let mut graph = mono::session(&[looping, silent]);
    let mut parent = graph.loops[0].clone();
    parent.id = Identity {
        slot: 2,
        generation: 1,
    };
    parent.strip = None;
    let parent_id = parent.id;
    graph.loops.push(parent);
    let quantum = u64::from(limits.quantum);
    for (revision, iterations) in [(3, 2), (4, 5)] {
        graph.loops[2].composite = Some(CompositeView {
            kind: CompositeKind::Regular,
            version: revision - 2,
            unit_frames: quantum,
            iterations,
            sample_rate: 48_000,
            quantum: limits.quantum,
            spans: vec![CompositeSpan {
                target: identity,
                start: 0,
                end: if revision == 3 { 2 } else { 3 },
                occurrence_start: 0,
                ordinal: 0,
                mode: LoopMode::Playing,
            }]
            .into(),
        });
        let program = compile_composites(&graph, context, 32).unwrap();
        let layout = allocator.stage(&program).unwrap();
        let parent = layout.loop_id(parent_id).unwrap();
        let media = layout.media_views(&program);
        let image = RtlTarget {
            capacities: limits,
            media_views: &media,
            state_base: 0,
        }
        .build_image(&program, layout)
        .unwrap();
        let requested = stage(&mut session, Upload::new(image, limits.instructions));
        allocator.publish(revision).unwrap();
        let (active, retired) = session.activate(requested, timeout()).unwrap();
        assert_eq!(active.frame, (if revision == 3 { 6 } else { 10 }) * quantum);
        assert_eq!(retired.unwrap().image.layout.revision, revision - 1);
        allocator.complete(revision).unwrap();
        if revision == 3 {
            let mut window = ControlWindow::begin(session.client(), revision, timeout()).unwrap();
            let play = TransitionPlan {
                now: Some(TransitionMode::Playing),
                ..Default::default()
            };
            assert_eq!(
                window.apply_transition(parent, None, None, play),
                Ok(Code::Applied)
            );
            window.release().unwrap();
            for _ in 0..4 {
                send(session.client(), Request::run_quantum());
            }
        } else {
            let edited = graph.loops[2].composite.as_mut().unwrap();
            edited.version += 1;
            edited.spans.to_mut()[0].end = iterations;
            let future = compile_composites(&graph, context, 32).unwrap();
            for revision in [5, 6] {
                let layout = allocator.stage(&future).unwrap();
                let media = layout.media_views(&future);
                let image = RtlTarget {
                    capacities: limits,
                    media_views: &media,
                    state_base: 0,
                }
                .build_image(&future, layout)
                .unwrap();
                let replacement = stage(&mut session, Upload::new(image, limits.instructions));
                allocator.publish(revision).unwrap();
                if revision == 5 {
                    assert!(matches!(
                        session.activate(replacement, timeout()),
                        Err(Error::Rejected(RESULT_REJECTED))
                    ));
                    send(session.client(), Request::run_quantum());
                    let capture = send(session.client(), Request::capture_loop(parent.slot));
                    let state = send(session.client(), Request::read_snapshot(capture[0], 0, 4));
                    assert_eq!((state[1] >> 1) & 3, 1);
                    assert_eq!(u64::from(state[4]) | (u64::from(state[5]) << 32), quantum);
                    assert_eq!(
                        u64::from(state[6]) | (u64::from(state[7]) << 32),
                        iterations * quantum
                    );
                    let passes = send(session.client(), Request::read_snapshot(capture[0], 5, 1));
                    assert_eq!(passes[..2], [2, 0]);
                }
                let (active, retired) = session.activate(replacement, timeout()).unwrap();
                assert_eq!(active.frame, 11 * quantum);
                assert_eq!(retired.unwrap().image.layout.revision, revision - 1);
                allocator.complete(revision).unwrap();
            }
        }
    }

    let voice = session.active().unwrap().layout.loop_id(identity).unwrap();
    graph.loops.pop();
    let program = compile_composites(&graph, context, 32).unwrap();
    let layout = allocator
        .stage_with_composite_retirement(&program, true)
        .unwrap();
    let departing = layout.retiring_composite().unwrap();
    let media = layout.media_views(&program);
    let image = RtlTarget {
        capacities: limits,
        media_views: &media,
        state_base: 0,
    }
    .build_image(&program, layout)
    .unwrap();
    let requested = stage(&mut session, Upload::new(image, limits.instructions));
    allocator.publish(7).unwrap();
    let mut window = ControlWindow::begin(session.client(), 6, timeout()).unwrap();
    assert_eq!(
        window.prepare_composite_retirement(departing),
        Ok(Code::Scheduled)
    );
    window.release().unwrap();
    send(session.client(), Request::run_quantum());
    let capture = send(session.client(), Request::capture_loop(voice.slot));
    let state = send(session.client(), Request::read_snapshot(capture[0], 0, 1));
    assert_eq!((state[1] >> 1) & 3, 0);
    let (active, retired) = session.activate(requested, timeout()).unwrap();
    assert_eq!(
        active,
        ActiveProgram {
            frame: 12 * quantum,
            revision: 7
        }
    );
    assert_eq!(retired.unwrap().image.layout.revision, 6);
    allocator.complete(7).unwrap();
}
