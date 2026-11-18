mod support;
use rt_fpga_compiler::{
    graph::{compile, shoop_graph::*, CompileContext},
    *,
};
use shoop_wasm_test_support::shoop_test;
use support::*;

fn context() -> CompileContext {
    CompileContext {
        quantum: 32,
        limits: ProgramLimits::default(),
    }
}
#[shoop_test]
fn channel_topologies_have_stable_state_and_deterministic_single_write_programs() {
    for channels in [1, 2, 4, 8, 64] {
        let mut g = session(channels);
        g.loops[0].sync_source = Some(g.loops[2].id);
        g.loops[0].boundary_smoothing_frames = 33;
        g.loops[1].balance = 0.25;
        g.strips[0].controls.input_gain_db = -6.;
        g.strips[0].controls.output_balance = -0.5;
        g.buses[0].gain_db = -3.;
        g.buses[1].muted = true;
        let program = compile(&g, context()).unwrap();
        assert_eq!(
            program
                .initial_state
                .iter()
                .filter(|s| matches!(s, InitialValue::MediaView(_)))
                .count(),
            3 * channels as usize
        );
        assert_eq!(program.resources.history_slots, channels);
        assert!(
            program
                .sync_pass
                .iter()
                .position(|&i| i == id(102))
                .unwrap()
                < program
                    .sync_pass
                    .iter()
                    .position(|&i| i == id(100))
                    .unwrap()
        );
        let mut written = std::collections::BTreeSet::new();
        for op in &program.audio_pass {
            let destination = match *op {
                Operation::Silence { destination }
                | Operation::Input { destination, .. }
                | Operation::Playback { destination, .. }
                | Operation::Gain { destination, .. }
                | Operation::Mix { destination, .. } => destination,
                Operation::Smoothing { control, .. } => {
                    match program.initial_state[control as usize] {
                        InitialValue::Smoothing { destination, .. } => destination,
                        _ => unreachable!(),
                    }
                }
                _ => continue,
            };
            assert!(written.insert(destination), "scratch buffer written twice");
        }
        g.strips.reverse();
        g.loops.reverse();
        g.buses.reverse();
        g.ports.reverse();
        g.connections.reverse();
        assert_eq!(compile(&g, context()).unwrap(), program);
    }
}
#[shoop_test]
fn arbitrary_routes_include_cross_channel_fan_in_fan_out_and_loop_ports() {
    let mut g = session(4);
    let source = g
        .ports
        .iter()
        .find(|p| {
            p.id.owner == Owner::Strip(id(10)) && p.id.role == PortRole::Output && p.channel == 3
        })
        .unwrap()
        .id;
    let destinations = g
        .ports
        .iter()
        .filter(|p| p.id.owner == Owner::Bus(id(21)) && p.id.role == PortRole::Input)
        .map(|p| p.id)
        .collect::<Vec<_>>();
    for destination in destinations {
        edge(&mut g, source, destination);
    }
    let loop_out = port(&mut g, Owner::Loop(id(102)), PortRole::Output, 2);
    let loop_in = port(&mut g, Owner::Loop(id(100)), PortRole::Input, 0);
    edge(&mut g, loop_out, loop_in);
    compile(&g, context()).unwrap();
    let master_out = g
        .ports
        .iter()
        .find(|p| {
            p.id.owner == Owner::Bus(id(21)) && p.id.role == PortRole::Output && p.channel == 0
        })
        .unwrap()
        .id;
    let aux_in = g
        .ports
        .iter()
        .find(|p| {
            p.id.owner == Owner::Bus(id(20)) && p.id.role == PortRole::Input && p.channel == 0
        })
        .unwrap()
        .id;
    edge(&mut g, master_out, aux_in);
    assert_eq!(compile(&g, context()), Err(CompileError::InvalidProgram));
}
#[shoop_test]
fn target_resource_admission_includes_channels_smoothing_and_all_emitted_operations() {
    let mut g = session(4);
    g.loops[0].boundary_smoothing_frames = 33;
    let p = compile(&g, context()).unwrap();
    let exact = ProgramLimits {
        instructions: (p.sync_pass.len() + p.audio_pass.len()) as u32,
        state_values: p.initial_state.len() as u32,
        loop_slots: 3,
        jobs: p.resources.queue_capacity,
        buffers: p.resources.buffers,
        history_slots: p.resources.history_slots,
        input_ports: p.resources.input_ports,
        output_ports: p.resources.output_ports,
        quantum: 32,
    };
    compile(
        &g,
        CompileContext {
            quantum: 32,
            limits: exact,
        },
    )
    .unwrap();
    for resource in [
        Resource::Instructions,
        Resource::State,
        Resource::Loops,
        Resource::Jobs,
        Resource::Buffers,
        Resource::History,
        Resource::Inputs,
        Resource::Outputs,
        Resource::Quantum,
    ] {
        let mut limits = exact;
        match resource {
            Resource::Instructions => limits.instructions -= 1,
            Resource::State => limits.state_values -= 1,
            Resource::Loops => limits.loop_slots -= 1,
            Resource::Jobs => limits.jobs -= 1,
            Resource::Buffers => limits.buffers -= 1,
            Resource::History => limits.history_slots -= 1,
            Resource::Inputs => limits.input_ports -= 1,
            Resource::Outputs => limits.output_ports -= 1,
            Resource::Quantum => limits.quantum -= 1,
        }
        assert_eq!(
            compile(
                &g,
                CompileContext {
                    quantum: 32,
                    limits
                }
            ),
            Err(CompileError::Capacity(resource))
        );
    }
}
#[shoop_test]
fn empty_sessions_and_monitoring_without_loops_are_real_programs() {
    compile(&Graph::default(), context()).unwrap();
    let mut g = session(2);
    g.loops.clear();
    let p = compile(&g, context()).unwrap();
    assert!(p.loops.is_empty());
    assert!(p
        .audio_pass
        .iter()
        .any(|op| matches!(op, Operation::Input { .. })));
    assert!(p
        .audio_pass
        .iter()
        .any(|op| matches!(op, Operation::Output { .. })));
}
#[shoop_test]
fn malformed_channel_positions_and_layouts_are_rejected() {
    let g = session(2);
    for mutation in 0..5 {
        let mut invalid = g.clone();
        match mutation {
            0 => {
                invalid.loops[0].layout = ChannelLayout::Direct {
                    audio: 4,
                    midi: false,
                }
            }
            1 => invalid.ports[2].channel = 10,
            2 => {
                let mut port = invalid.ports[0].clone();
                port.id.index += 1000;
                invalid.ports.push(port);
            }
            3 => invalid.buses[0].gain_db = f32::NAN,
            _ => invalid.loops[0].sync_source = Some(id(999)),
        }
        assert_eq!(
            compile(&invalid, context()),
            Err(CompileError::InvalidProgram)
        );
    }
}
