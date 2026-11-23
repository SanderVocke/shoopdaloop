#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
#[path = "../../rt_fpga_compiler/tests/support/mod.rs"]
mod support;
use rt_fpga_compiler::shoop_graph::{Graph, Owner, PortRole};
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock},
    host::Renderer,
};
use shoop_rt_protocol::{
    immediate::{
        Batch, Code, Command, IterationTransition, PlannedTransition, Response, TransitionMode,
        TransitionPlan,
    },
    DriverKind, DriverRunState, DriverStatus, EngineMessage, LoopSlotId, SlotId,
};
use shoop_rt_transport::{
    carrier::{Endpoint, InProcess, Outbox},
    client::ProtocolClient,
};
use shoop_wasm_test_support::shoop_test;
use std::time::{Duration, Instant};
use support::*;
const Q: usize = 32;
fn step(r: &mut impl Renderer) -> [f32; 2] {
    step_with_input(r, 0.25)
}

fn step_with_input(r: &mut impl Renderer, input: f32) -> [f32; 2] {
    let mut a = [0.; Q];
    let mut b = a;
    r.process(
        AudioBlock {
            frame: r.next_frame(),
            frames: Q,
            inputs: &[&[input; Q]],
            outputs: &mut [&mut a, &mut b],
        },
        MidiBlock {
            inputs: &[],
            outputs: &mut [],
        },
    )
    .unwrap();
    assert!(a.iter().all(|&s| s == a[0]) && b.iter().all(|&s| s == b[0]));
    [a[0], b[0]]
}
fn response<E: Endpoint>(c: &mut ProtocolClient<InProcess<E>>, r: &mut impl Renderer) -> u64 {
    let response = wait_response(c, r);
    assert_eq!(response.header.code(), Code::Applied);
    for record in response.records() {
        assert_eq!(record.code(), Code::Applied, "{:?}", record.command());
    }
    response.header.frame()
}

fn wait_response<E: Endpoint>(
    c: &mut ProtocolClient<InProcess<E>>,
    r: &mut impl Renderer,
) -> Response {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        while let Some(m) = c.poll() {
            if let EngineMessage::ImmediateResponse(response) = m {
                return response;
            }
        }
        assert!(Instant::now() < deadline, "graph command stalled");
        step(r);
    }
}
fn upload<E: Endpoint>(
    c: &mut ProtocolClient<InProcess<E>>,
    r: &mut impl Renderer,
    g: &Graph,
    revision: u64,
) -> u64 {
    let mut frame = 0;
    for batch in shoop_rt_protocol::graph::upload(g, Q as u32, revision).unwrap() {
        c.immediate(batch).unwrap();
        frame = response(c, r);
    }
    frame
}
fn stereo_graph() -> Graph {
    let mut graph = session(2);
    for s in &mut graph.strips {
        s.controls.input_monitoring = false;
    }
    let first = graph
        .ports
        .iter()
        .find(|p| p.id.owner == Owner::Host && p.id.role == PortRole::Input && p.channel == 0)
        .unwrap()
        .id;
    let second = graph
        .ports
        .iter()
        .find(|p| p.id.owner == Owner::Host && p.id.role == PortRole::Input && p.channel == 1)
        .unwrap()
        .id;
    for edge in &mut graph.connections {
        if edge.source == second {
            edge.source = first;
        }
    }
    graph.ports.retain(|p| p.id != second);
    graph
}

fn run<E: Endpoint>(endpoint: E, mut renderer: impl Renderer) -> Vec<[f32; 2]> {
    let mut c = ProtocolClient::new(InProcess::new(endpoint, Outbox::new(8, 8, 0)));
    c.connect().unwrap();
    while c.poll().is_some() {}
    let mut graph = stereo_graph();
    graph.loops[0].balance = 0.5;
    upload(&mut c, &mut renderer, &graph, 0);
    for mode in [TransitionMode::Recording, TransitionMode::Playing] {
        let commands = (100..103)
            .map(|slot| {
                Command::Transition(
                    slot,
                    TransitionPlan {
                        now: Some(mode),
                        first: None,
                        second: None,
                    },
                )
            })
            .collect::<Vec<_>>();
        c.immediate(Batch::new(1, 1, &commands).unwrap()).unwrap();
        response(&mut c, &mut renderer);
        for _ in 0..8 {
            step(&mut renderer);
            while c.poll().is_some() {}
        }
    }
    let first = step(&mut renderer);
    assert_eq!(first, [0.375, 0.4375]);
    graph.buses[0].muted = true;
    upload(&mut c, &mut renderer, &graph, 1);
    for _ in 0..4 {
        step(&mut renderer);
        while c.poll().is_some() {}
    }
    let next = step(&mut renderer);
    assert_eq!(next, [0.25, 0.25]);
    vec![first, next]
}
#[shoop_test(no_wasm = "actual Marlin simulation and reference renderer")]
fn complete_stereo_graphs_record_play_and_retain_media_on_both_backends() {
    let driver = DriverStatus {
        kind: DriverKind::Dummy,
        state: DriverRunState::Running,
        sample_rate: 48000,
        buffer_frames: Q as u32,
    };
    let p = shoop_rt_host_reference::prepare(Q as u32, driver, 1).unwrap();
    let expected = run(p.endpoint, p.renderer);
    let p = shoop_rt_host_marlin::prepare(Q as u32, driver, 1, Q).unwrap();
    assert_eq!(run(p.endpoint, p.renderer), expected);
}

#[shoop_test(no_wasm = "actual Marlin simulation")]
fn default_rtl_profile_runs_a_routed_64_loop_stereo_graph() {
    let driver = DriverStatus {
        kind: DriverKind::Dummy,
        state: DriverRunState::Running,
        sample_rate: 48000,
        buffer_frames: Q as u32,
    };
    let prepared = shoop_rt_host_marlin::prepare(Q as u32, driver, 256, Q).unwrap();
    let mut renderer = prepared.renderer;
    let mut client = ProtocolClient::new(InProcess::new(prepared.endpoint, Outbox::new(8, 8, 0)));
    client.connect().unwrap();
    while client.poll().is_some() {}
    let mut graph = stereo_graph();
    let template = graph.loops[0].clone();
    graph.loops = (100..164)
        .map(|slot| rt_fpga_compiler::shoop_graph::LoopView {
            id: id(slot),
            ..template.clone()
        })
        .collect();
    upload(&mut client, &mut renderer, &graph, 0);
    assert_eq!(step(&mut renderer), [0., 0.]);
}

fn composite_round_trip<E: Endpoint>(endpoint: E, mut renderer: impl Renderer) {
    use rt_fpga_compiler::shoop_graph::{CompositeKind, CompositeSpan, CompositeView, LoopMode};
    let mut client = ProtocolClient::new(InProcess::new(endpoint, Outbox::new(8, 8, 0)));
    client.connect().unwrap();
    while client.poll().is_some() {}
    let mut graph = stereo_graph();
    graph.loops.truncate(1);
    let one_shot_iterations = 64;
    let mut parent = graph.loops[0].clone();
    parent.id = id(120);
    parent.strip = None;
    parent.composite = Some(CompositeView {
        kind: CompositeKind::Explicit,
        version: 1,
        unit_frames: Q as u64,
        iterations: one_shot_iterations,
        sample_rate: 48000,
        quantum: Q as u32,
        spans: vec![
            CompositeSpan {
                target: id(100),
                start: 0,
                end: 2,
                occurrence_start: 0,
                ordinal: 0,
                mode: LoopMode::Recording,
            },
            CompositeSpan {
                target: id(100),
                start: 2,
                end: one_shot_iterations,
                occurrence_start: 2,
                ordinal: 1,
                mode: LoopMode::Playing,
            },
        ]
        .into(),
    });
    graph.loops.push(parent);
    let mut wrong_rate = graph.clone();
    wrong_rate.loops[1].composite.as_mut().unwrap().sample_rate = 44_100;
    for batch in shoop_rt_protocol::graph::upload(&wrong_rate, Q as u32, 0).unwrap() {
        let last = batch.commands().contains(&Command::ActivateGraph);
        client.immediate(batch).unwrap();
        if last {
            assert_eq!(
                wait_response(&mut client, &mut renderer).header.code(),
                Code::InvalidValue
            );
        } else {
            response(&mut client, &mut renderer);
        }
    }
    upload(&mut client, &mut renderer, &graph, 0);
    let transition = |slot, mode| {
        Command::Transition(
            slot,
            TransitionPlan {
                now: Some(mode),
                first: None,
                second: None,
            },
        )
    };
    client
        .immediate(Batch::new(1, 1, &[transition(120, TransitionMode::Playing)]).unwrap())
        .unwrap();
    let one_shot_started = response(&mut client, &mut renderer);
    assert!((0..8).any(|_| step(&mut renderer) == [0.125, 0.125]));

    let plan = graph.loops[1].composite.as_mut().unwrap();
    plan.kind = CompositeKind::Regular;
    plan.version = 2;
    plan.iterations = 16;
    plan.spans = vec![CompositeSpan {
        target: id(100),
        start: 0,
        end: 16,
        occurrence_start: 0,
        ordinal: 0,
        mode: LoopMode::Playing,
    }]
    .into();
    let switched = upload(&mut client, &mut renderer, &graph, 1);
    assert_eq!(
        switched,
        one_shot_started + (one_shot_iterations + 1) * Q as u64
    );
    assert_eq!(step(&mut renderer), [0., 0.]);
    client
        .immediate(Batch::new(2, 2, &[transition(120, TransitionMode::Playing)]).unwrap())
        .unwrap();
    let start = response(&mut client, &mut renderer);
    assert_eq!(step(&mut renderer), [0.125, 0.125]);

    let plan = graph.loops[1].composite.as_mut().unwrap();
    plan.version = 3;
    let span = &mut plan.spans.to_mut()[0];
    span.start = 4;
    span.occurrence_start = 4;
    let switched = upload(&mut client, &mut renderer, &graph, 2);
    assert!(switched > start);
    assert_eq!((switched - start) % (16 * Q as u64), 0);
    let mut silent = 0;
    for _ in 0..16 {
        let samples = step(&mut renderer);
        if samples == [0., 0.] {
            silent += 1;
        } else {
            assert_eq!(samples, [0.125, 0.125]);
        }
    }
    assert_eq!(silent, 4);

    let iteration = |iteration, seek| {
        Command::LoopIteration(
            LoopSlotId(SlotId::new(120, id(120).generation)),
            IterationTransition {
                iteration,
                seek,
                retrigger: false,
            },
        )
    };
    for (position, expected) in [(0, [0., 0.]), (4, [0.125, 0.125])] {
        client
            .immediate(Batch::new(3, 3, &[iteration(position, true)]).unwrap())
            .unwrap();
        response(&mut client, &mut renderer);
        assert_eq!(step(&mut renderer), expected);
    }
    client
        .immediate(Batch::new(3, 3, &[iteration(16, true)]).unwrap())
        .unwrap();
    let rejected = wait_response(&mut client, &mut renderer);
    assert_eq!(rejected.records()[0].code(), Code::InvalidValue);
    assert_eq!(step(&mut renderer), [0.125, 0.125]);
    let Command::LoopIteration(_, request) = iteration(0, true) else {
        unreachable!();
    };
    let stale = Command::LoopIteration(
        LoopSlotId(SlotId::new(120, id(120).generation + 1)),
        request,
    );
    client
        .immediate(Batch::new(3, 3, &[stale]).unwrap())
        .unwrap();
    let rejected = wait_response(&mut client, &mut renderer);
    assert_eq!(rejected.records()[0].code(), Code::TargetUnavailable);
    assert_eq!(step(&mut renderer), [0.125, 0.125]);

    client
        .immediate(Batch::new(3, 3, &[transition(120, TransitionMode::Stopped)]).unwrap())
        .unwrap();
    response(&mut client, &mut renderer);
    assert_eq!(step(&mut renderer), [0., 0.]);
    client
        .immediate(Batch::new(3, 3, &[iteration(6, false)]).unwrap())
        .unwrap();
    response(&mut client, &mut renderer);
    assert_eq!(step(&mut renderer), [0.125, 0.125]);
    let stop_on_wrap = Command::Transition(
        120,
        TransitionPlan {
            now: None,
            first: Some(PlannedTransition {
                mode: TransitionMode::Stopped,
                skip: 0,
            }),
            second: None,
        },
    );
    client
        .immediate(Batch::new(3, 3, &[stop_on_wrap]).unwrap())
        .unwrap();
    assert_eq!(
        wait_response(&mut client, &mut renderer).records()[0].code(),
        Code::Scheduled
    );
    for _ in 0..16 {
        step(&mut renderer);
    }
    for _ in 0..16 {
        assert_eq!(step(&mut renderer), [0., 0.]);
    }
    let mut clock = graph.loops[0].clone();
    clock.id = id(101);
    clock.gain = 0.;
    graph.loops[1].sync_source = Some(clock.id);
    graph.loops.push(clock);
    upload(&mut client, &mut renderer, &graph, 3);
    client
        .immediate(Batch::new(4, 4, &[transition(101, TransitionMode::Recording)]).unwrap())
        .unwrap();
    response(&mut client, &mut renderer);
    for _ in 0..24 {
        assert_eq!(step(&mut renderer), [0., 0.]);
    }
    let repeat = |enabled| {
        Command::LoopRepeatSync(LoopSlotId(SlotId::new(120, id(120).generation)), enabled)
    };
    client
        .immediate(
            Batch::new(
                4,
                4,
                &[
                    transition(101, TransitionMode::Playing),
                    repeat(true),
                    iteration(14, false),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    response(&mut client, &mut renderer);
    assert_eq!(step(&mut renderer), [0.125, 0.125]);
    step(&mut renderer);
    for _ in 0..8 {
        assert_eq!(step(&mut renderer), [0., 0.]);
    }
    client
        .immediate(Batch::new(4, 4, &[repeat(false)]).unwrap())
        .unwrap();
    response(&mut client, &mut renderer);
    assert!((0..7).any(|_| step(&mut renderer) == [0.125, 0.125]));
    client
        .immediate(
            Batch::new(
                4,
                4,
                &[
                    transition(120, TransitionMode::Stopped),
                    transition(101, TransitionMode::Stopped),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    response(&mut client, &mut renderer);
    graph.loops.truncate(1);
    upload(&mut client, &mut renderer, &graph, 4);
    client
        .immediate(Batch::new(5, 5, &[transition(100, TransitionMode::Playing)]).unwrap())
        .unwrap();
    response(&mut client, &mut renderer);
    assert_eq!(step(&mut renderer), [0.125, 0.125]);
}

fn composite_conversion<E: Endpoint>(endpoint: E, mut renderer: impl Renderer) {
    use rt_fpga_compiler::shoop_graph::{CompositeKind, CompositeView};
    let mut client = ProtocolClient::new(InProcess::new(endpoint, Outbox::new(8, 8, 0)));
    client.connect().unwrap();
    while client.poll().is_some() {}
    let mut graph = stereo_graph();
    graph.loops.truncate(2);
    upload(&mut client, &mut renderer, &graph, 0);

    let convert = |graph: &Graph, index: usize| {
        let mut candidate = graph.clone();
        let row = &mut candidate.loops[index];
        row.id.generation += 1;
        row.composite = Some(CompositeView {
            kind: CompositeKind::Regular,
            version: 1,
            unit_frames: Q as u64,
            iterations: 0,
            sample_rate: 48000,
            quantum: Q as u32,
            spans: Vec::new().into(),
        });
        candidate
    };
    let transition = |mode: TransitionMode| Command::Transition(100, mode.into());
    let refused = convert(&graph, 0);
    let batches = shoop_rt_protocol::graph::upload(&refused, Q as u32, 1).unwrap();
    for batch in &batches[..batches.len() - 1] {
        client.immediate(*batch).unwrap();
        response(&mut client, &mut renderer);
    }
    // Recording starts after uploading the candidate and before its ownership fence.
    client
        .immediate(Batch::new(1, 1, &[transition(TransitionMode::Recording)]).unwrap())
        .unwrap();
    response(&mut client, &mut renderer);
    step(&mut renderer);
    client.immediate(*batches.last().unwrap()).unwrap();
    assert_eq!(
        wait_response(&mut client, &mut renderer).header.code(),
        Code::CannotApply
    );
    client
        .immediate(Batch::new(1, 1, &[transition(TransitionMode::Stopped)]).unwrap())
        .unwrap();
    response(&mut client, &mut renderer);
    for batch in batches {
        let activation = batch.commands().contains(&Command::ActivateGraph);
        client.immediate(batch).unwrap();
        if activation {
            assert_eq!(
                wait_response(&mut client, &mut renderer).header.code(),
                Code::CannotApply
            );
        } else {
            response(&mut client, &mut renderer);
        }
    }
    upload(&mut client, &mut renderer, &convert(&graph, 1), 1);
    client
        .immediate(Batch::new(4, 4, &[transition(TransitionMode::Playing)]).unwrap())
        .unwrap();
    response(&mut client, &mut renderer);
    assert_eq!(step(&mut renderer), [0.125, 0.125]);
}

fn future_composite_child<E: Endpoint>(endpoint: E, mut renderer: impl Renderer, nested: bool) {
    use rt_fpga_compiler::shoop_graph::{CompositeKind, CompositeSpan, CompositeView, LoopMode};
    let mut client = ProtocolClient::new(InProcess::new(endpoint, Outbox::new(8, 8, 0)));
    client.connect().unwrap();
    while client.poll().is_some() {}
    let mut graph = stereo_graph();
    graph.loops.truncate(2);
    let mut parent = graph.loops[0].clone();
    parent.id = id(120);
    parent.strip = None;
    let play = |child, start, ordinal| CompositeSpan {
        target: id(child),
        start,
        end: 128,
        occurrence_start: start,
        ordinal,
        mode: LoopMode::Playing,
    };
    parent.composite = Some(CompositeView {
        kind: CompositeKind::Regular,
        version: 1,
        unit_frames: Q as u64,
        iterations: 128,
        sample_rate: 48000,
        quantum: Q as u32,
        spans: vec![play(100, 0, 0)].into(),
    });
    graph.loops.push(parent);
    let playing = if nested {
        let mut outer = graph.loops[2].clone();
        outer.id = id(121);
        outer.composite.as_mut().unwrap().spans = vec![play(120, 0, 0)].into();
        graph.loops.push(outer);
        121
    } else {
        120
    };
    upload(&mut client, &mut renderer, &graph, 0);
    let transition = |slot, mode: TransitionMode| Command::Transition(slot, mode.into());
    for mode in [TransitionMode::Recording, TransitionMode::Stopped] {
        let commands = [transition(100, mode), transition(101, mode)];
        client
            .immediate(Batch::new(1, 1, &commands).unwrap())
            .unwrap();
        response(&mut client, &mut renderer);
        step(&mut renderer);
    }
    client
        .immediate(Batch::new(1, 1, &[transition(playing, TransitionMode::Playing)]).unwrap())
        .unwrap();
    let started = response(&mut client, &mut renderer);
    assert_eq!(step(&mut renderer), [0.125, 0.125]);
    let program = graph.loops[2].composite.as_mut().unwrap();
    program.version += 1;
    program.spans.to_mut().push(play(101, 96, 1));
    let switched = upload(&mut client, &mut renderer, &graph, 1);
    assert!(switched < started + 95 * Q as u64);
    // Both dummy-driver paths deliver the block buffered by the quantum adapter.
    let audible_start = started + Q as u64;
    let mut second_child_heard = false;
    loop {
        let frame = renderer.next_frame().0;
        if frame >= audible_start + 128 * Q as u64 {
            break;
        }
        let sample = if frame < audible_start + 96 * Q as u64 {
            0.125
        } else {
            second_child_heard = true;
            0.1875
        };
        assert_eq!(step(&mut renderer), [sample, sample], "frame {frame}");
    }
    assert!(second_child_heard);
}

#[shoop_test(no_wasm = "actual Marlin simulation and reference renderer")]
fn a_playing_composite_can_add_a_child_later_in_the_current_pass() {
    let driver = DriverStatus {
        kind: DriverKind::Dummy,
        state: DriverRunState::Running,
        sample_rate: 48000,
        buffer_frames: Q as u32,
    };
    for nested in [false, true] {
        let rust = shoop_rt_host_reference::prepare(Q as u32, driver, 1).unwrap();
        future_composite_child(rust.endpoint, rust.renderer, nested);
        let rtl = shoop_rt_host_marlin::prepare(Q as u32, driver, 1, Q).unwrap();
        future_composite_child(rtl.endpoint, rtl.renderer, nested);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ending {
    Same,
    AddSecond,
    ReplaceFirst,
}

#[derive(Clone, Copy)]
enum Clock {
    Primitive,
    Composite,
    RelayedComposite,
    AlignedComposite,
}

fn edit_at_clock_restart<E: Endpoint>(
    endpoint: E,
    mut renderer: impl Renderer,
    iterations: u64,
    unit_quanta: u64,
    ending: Ending,
    clock: Clock,
    stop_at_pulse: bool,
) {
    use rt_fpga_compiler::shoop_graph::{CompositeKind, CompositeSpan, CompositeView, LoopMode};
    let mut client = ProtocolClient::new(InProcess::new(endpoint, Outbox::new(8, 8, 0)));
    client.connect().unwrap();
    while client.poll().is_some() {}
    let mut graph = stereo_graph();
    graph
        .loops
        .truncate(if ending == Ending::Same { 2 } else { 3 });
    graph.loops[1].gain = 0.;
    let parent_index = graph.loops.len();
    let clock_slot = match clock {
        Clock::Primitive | Clock::RelayedComposite => 101,
        Clock::Composite | Clock::AlignedComposite => 121,
    };
    let mut parent = graph.loops[0].clone();
    parent.id = id(120);
    parent.strip = None;
    parent.sync_source = Some(id(clock_slot));
    parent.repeat_sync = true;
    parent.composite = Some(CompositeView {
        kind: CompositeKind::Regular,
        version: 1,
        unit_frames: Q as u64,
        iterations: 16,
        sample_rate: 48000,
        quantum: Q as u32,
        spans: vec![CompositeSpan {
            target: id(100),
            start: 0,
            end: 16,
            occurrence_start: 0,
            ordinal: 0,
            mode: LoopMode::Playing,
        }]
        .into(),
    });
    graph.loops.push(parent);
    if !matches!(clock, Clock::Primitive) {
        let mut composite = graph.loops[parent_index].clone();
        composite.id = id(121);
        composite.sync_source = matches!(clock, Clock::AlignedComposite).then_some(id(101));
        composite.repeat_sync = composite.sync_source.is_some();
        let (iterations, child) = match clock {
            Clock::Composite => (100, 101),
            Clock::RelayedComposite => (72, 103),
            Clock::AlignedComposite => (40, 103),
            Clock::Primitive => unreachable!(),
        };
        let program = composite.composite.as_mut().unwrap();
        program.iterations = iterations;
        program.spans = vec![CompositeSpan {
            target: id(child),
            start: 0,
            end: iterations,
            occurrence_start: 0,
            ordinal: 0,
            mode: LoopMode::Playing,
        }]
        .into();
        graph.loops.push(composite);
        if child == 103 {
            let mut silent = graph.loops[1].clone();
            silent.id = id(child);
            graph.loops.push(silent.clone());
            if matches!(clock, Clock::RelayedComposite) {
                silent.id = id(104);
                silent.sync_source = Some(id(121));
                graph.loops[1].sync_source = Some(silent.id);
                graph.loops.push(silent);
            }
        }
    }
    upload(&mut client, &mut renderer, &graph, 0);
    let transition = |slot, mode: TransitionMode| Command::Transition(slot, mode.into());
    let recorded = [(100, 4)]
        .into_iter()
        .chain((ending != Ending::Same).then_some((102, 0)))
        .chain([(101, 96)]);
    let record = recorded
        .clone()
        .map(|(slot, _)| transition(slot, TransitionMode::Recording))
        .collect::<Vec<_>>();
    client
        .immediate(Batch::new(1, 1, &record).unwrap())
        .unwrap();
    let capture_start = response(&mut client, &mut renderer);
    for (slot, ticks) in recorded {
        for _ in 0..ticks {
            assert_eq!(step(&mut renderer), [0., 0.]);
        }
        client
            .immediate(Batch::new(1, 1, &[transition(slot, TransitionMode::Stopped)]).unwrap())
            .unwrap();
        // The clock’s capture length is fixed when its Stop takes effect.
        if slot != 101 {
            response(&mut client, &mut renderer);
        }
    }
    let recorded_length = response(&mut client, &mut renderer) - capture_start;
    let (clock_length, clocks): (u64, &[u32]) = match clock {
        Clock::Primitive => (recorded_length, &[101]),
        Clock::Composite => (100 * Q as u64, &[121]),
        Clock::RelayedComposite => (72 * Q as u64, &[121, 101]),
        Clock::AlignedComposite => (recorded_length, &[101, 121]),
    };
    let play = clocks
        .iter()
        .copied()
        .chain([120])
        .map(|slot| transition(slot, TransitionMode::Playing))
        .collect::<Vec<_>>();
    client.immediate(Batch::new(1, 1, &play).unwrap()).unwrap();
    let started = response(&mut client, &mut renderer);
    assert_eq!(step(&mut renderer), [0.125, 0.125]);
    for _ in 0..20 {
        step(&mut renderer);
    }
    assert_eq!(step(&mut renderer), [0., 0.]);

    let edit = graph.loops[parent_index].composite.as_mut().unwrap();
    edit.version = 2;
    edit.iterations = iterations;
    edit.unit_frames = unit_quanta * Q as u64;
    if ending == Ending::ReplaceFirst {
        edit.spans.to_mut().clear();
    } else {
        let span = &mut edit.spans.to_mut()[0];
        span.start = 4;
        span.occurrence_start = 4;
        span.end = if ending == Ending::AddSecond {
            iterations - 2
        } else {
            iterations
        };
    }
    if ending != Ending::Same {
        edit.spans.to_mut().push(CompositeSpan {
            target: id(102),
            start: 6,
            end: iterations,
            occurrence_start: 6,
            ordinal: 1,
            mode: LoopMode::Playing,
        });
    }
    let key = 41;
    let batches = shoop_rt_protocol::graph::upload_composite(&graph, Q as u32, 1, key).unwrap();
    for (index, batch) in batches.iter().enumerate() {
        client.immediate(*batch).unwrap();
        if index + 1 < batches.len() {
            response(&mut client, &mut renderer);
        }
    }
    for _ in 0..8 {
        assert_eq!(step(&mut renderer), [0., 0.]);
        while let Some(message) = client.poll() {
            assert!(
                !matches!(message, EngineMessage::ImmediateResponse(_)),
                "edit answered before the clock pulse"
            );
        }
    }
    if stop_at_pulse {
        let stop = Command::Transition(
            120,
            TransitionPlan {
                now: None,
                first: Some(PlannedTransition {
                    mode: TransitionMode::Stopped,
                    skip: 0,
                }),
                second: None,
            },
        );
        client
            .immediate(Batch::new(1, 1, &[stop]).unwrap())
            .unwrap();
        let accepted = wait_response(&mut client, &mut renderer);
        assert_eq!(accepted.records()[0].command(), stop);
        assert_eq!(accepted.records()[0].code(), Code::Scheduled);
        let switched = response(&mut client, &mut renderer);
        assert_eq!(switched, started + clock_length + Q as u64);
        for _ in 0..16 {
            assert_eq!(step(&mut renderer), [0., 0.]);
        }
        return;
    }
    let switched = response(&mut client, &mut renderer);
    assert_eq!(switched, started + clock_length);
    let cancel = Command::CancelCompositeGraph(key);
    client
        .immediate(Batch::new(2, 2, &[cancel]).unwrap())
        .unwrap();
    assert_eq!(
        wait_response(&mut client, &mut renderer).header.code(),
        Code::TargetUnavailable
    );
    let audible_restart = switched + Q as u64;
    let starts = audible_restart + 4 * unit_quanta * Q as u64;
    let stops = audible_restart + iterations * unit_quanta * Q as u64;
    assert!(renderer.next_frame().0 < starts);
    while renderer.next_frame().0 <= stops {
        let frame = renderer.next_frame().0;
        let unit = unit_quanta * Q as u64;
        let first_end = if ending == Ending::AddSecond {
            stops - 2 * unit
        } else {
            stops
        };
        let first = if ending != Ending::ReplaceFirst && (starts..first_end).contains(&frame) {
            0.125
        } else {
            0.
        };
        let second =
            if ending != Ending::Same && (audible_restart + 6 * unit..stops).contains(&frame) {
                0.25
            } else {
                0.
            };
        let sample = first + second;
        assert_eq!(step(&mut renderer), [sample, sample], "frame {frame}");
    }
    if ending == Ending::ReplaceFirst {
        client
            .immediate(Batch::new(2, 2, &[transition(100, TransitionMode::Playing)]).unwrap())
            .unwrap();
        response(&mut client, &mut renderer);
        assert_eq!(step(&mut renderer), [0.125, 0.125]);
    }
}

#[shoop_test(no_wasm = "actual Marlin simulation and reference renderer")]
fn a_repeat_held_edit_applies_at_the_independent_clock_pulse() {
    let driver = DriverStatus {
        kind: DriverKind::Dummy,
        state: DriverRunState::Running,
        sample_rate: 48000,
        buffer_frames: Q as u32,
    };
    for (iterations, unit_quanta, ending, clock, stop_at_pulse) in [
        (16, 1, Ending::Same, Clock::Primitive, false),
        (24, 1, Ending::Same, Clock::Primitive, false),
        (12, 2, Ending::Same, Clock::Primitive, false),
        (24, 1, Ending::AddSecond, Clock::Primitive, false),
        (16, 1, Ending::ReplaceFirst, Clock::Primitive, false),
        (24, 1, Ending::Same, Clock::Composite, false),
        (24, 1, Ending::ReplaceFirst, Clock::Composite, false),
        (24, 1, Ending::Same, Clock::Primitive, true),
        (24, 1, Ending::ReplaceFirst, Clock::RelayedComposite, false),
        (24, 1, Ending::Same, Clock::AlignedComposite, false),
    ] {
        let rust = shoop_rt_host_reference::prepare(Q as u32, driver, 1).unwrap();
        edit_at_clock_restart(
            rust.endpoint,
            rust.renderer,
            iterations,
            unit_quanta,
            ending,
            clock,
            stop_at_pulse,
        );
        let rtl = shoop_rt_host_marlin::prepare(Q as u32, driver, 1, Q).unwrap();
        edit_at_clock_restart(
            rtl.endpoint,
            rtl.renderer,
            iterations,
            unit_quanta,
            ending,
            clock,
            stop_at_pulse,
        );
    }
}

fn edit_one_shot<E: Endpoint>(endpoint: E, mut renderer: impl Renderer, scheduled_transport: bool) {
    use rt_fpga_compiler::shoop_graph::{CompositeKind, CompositeSpan, CompositeView, LoopMode};
    let mut client = ProtocolClient::new(InProcess::new(endpoint, Outbox::new(8, 8, 0)));
    client.connect().unwrap();
    while client.poll().is_some() {}
    let mut graph = stereo_graph();
    graph
        .loops
        .truncate(if scheduled_transport { 2 } else { 1 });
    if scheduled_transport {
        graph.loops[1].gain = 0.;
    }
    let parent_index = graph.loops.len();
    let mut parent = graph.loops[0].clone();
    parent.id = id(120);
    parent.strip = None;
    parent.sync_source = scheduled_transport.then_some(id(101));
    let old_length = 64;
    parent.composite = Some(CompositeView {
        kind: CompositeKind::Explicit,
        version: 1,
        unit_frames: Q as u64,
        iterations: old_length,
        sample_rate: 48000,
        quantum: Q as u32,
        spans: vec![CompositeSpan {
            target: id(100),
            start: 0,
            end: old_length,
            occurrence_start: 0,
            ordinal: 0,
            mode: LoopMode::Recording,
        }]
        .into(),
    });
    graph.loops.push(parent);
    upload(&mut client, &mut renderer, &graph, 0);
    let transition = |slot, mode: TransitionMode| Command::Transition(slot, mode.into());
    if scheduled_transport {
        client
            .immediate(Batch::new(1, 1, &[transition(101, TransitionMode::Recording)]).unwrap())
            .unwrap();
        let record_end = response(&mut client, &mut renderer) + old_length * Q as u64;
        while renderer.next_frame().0 < record_end {
            step(&mut renderer);
        }
        client
            .immediate(Batch::new(1, 1, &[transition(101, TransitionMode::Stopped)]).unwrap())
            .unwrap();
        assert_eq!(response(&mut client, &mut renderer), record_end);
    }
    let start = [
        transition(120, TransitionMode::Playing),
        transition(101, TransitionMode::Playing),
    ];
    client
        .immediate(Batch::new(1, 1, &start[..if scheduled_transport { 2 } else { 1 }]).unwrap())
        .unwrap();
    let started = response(&mut client, &mut renderer);
    let end_frame = started + old_length * Q as u64;
    if scheduled_transport {
        let restart = Command::Transition(
            120,
            TransitionPlan {
                first: Some(PlannedTransition {
                    mode: TransitionMode::Stopped,
                    skip: 0,
                }),
                second: Some(PlannedTransition {
                    mode: TransitionMode::Playing,
                    skip: 1,
                }),
                ..Default::default()
            },
        );
        client
            .immediate(Batch::new(1, 1, &[restart]).unwrap())
            .unwrap();
        assert_eq!(
            wait_response(&mut client, &mut renderer).records()[0].code(),
            Code::Scheduled
        );
    }
    let louder = 48;
    let edited = graph.loops[parent_index].composite.as_mut().unwrap();
    edited.version = 2;
    edited.iterations = if scheduled_transport { old_length } else { 24 };
    let span = &mut edited.spans.to_mut()[0];
    span.start = if scheduled_transport { 16 } else { 4 };
    span.occurrence_start = span.start;
    span.end = edited.iterations;
    let batches = shoop_rt_protocol::graph::upload_composite(&graph, Q as u32, 1, 43).unwrap();
    for (index, batch) in batches.iter().enumerate() {
        client.immediate(*batch).unwrap();
        if index + 1 < batches.len() {
            response(&mut client, &mut renderer);
        }
    }
    assert!(renderer.next_frame().0 < started + louder * Q as u64);
    while renderer.next_frame().0 < end_frame {
        let frame = renderer.next_frame().0;
        let input = if frame >= started + louder * Q as u64 {
            0.5
        } else {
            0.25
        };
        assert_eq!(step_with_input(&mut renderer, input), [0., 0.]);
        while let Some(message) = client.poll() {
            assert!(
                !matches!(message, EngineMessage::ImmediateResponse(_)),
                "edit applied before the take finished"
            );
        }
    }
    assert_eq!(response(&mut client, &mut renderer), end_frame);
    assert_eq!(step(&mut renderer), [0., 0.]);

    client
        .immediate(Batch::new(2, 2, &[transition(100, TransitionMode::Playing)]).unwrap())
        .unwrap();
    response(&mut client, &mut renderer);
    for iteration in 0..=old_length {
        let sample = if (louder..old_length).contains(&iteration) {
            0.25
        } else {
            0.125
        };
        assert_eq!(
            step(&mut renderer),
            [sample, sample],
            "take iteration {iteration}"
        );
    }
    if scheduled_transport {
        let next_take = end_frame + (old_length + 17) * Q as u64;
        while renderer.next_frame().0 < next_take {
            assert_eq!(step(&mut renderer), [0.125, 0.125]);
        }
        assert_eq!(step(&mut renderer), [0., 0.]);
    }
}

#[shoop_test(no_wasm = "actual Marlin simulation and reference renderer")]
fn a_compatible_one_shot_edit_applies_as_the_old_take_finishes() {
    let driver = DriverStatus {
        kind: DriverKind::Dummy,
        state: DriverRunState::Running,
        sample_rate: 48000,
        buffer_frames: Q as u32,
    };
    for scheduled_transport in [false, true] {
        let rust = shoop_rt_host_reference::prepare(Q as u32, driver, 1).unwrap();
        edit_one_shot(rust.endpoint, rust.renderer, scheduled_transport);
        let rtl = shoop_rt_host_marlin::prepare(Q as u32, driver, 1, Q).unwrap();
        edit_one_shot(rtl.endpoint, rtl.renderer, scheduled_transport);
    }
}

fn edit_at_continuous_wrap<E: Endpoint>(
    endpoint: E,
    mut renderer: impl Renderer,
    length: u64,
    drop_child: bool,
    other_parent: bool,
) {
    use rt_fpga_compiler::shoop_graph::{CompositeKind, CompositeSpan, CompositeView, LoopMode};
    let mut client = ProtocolClient::new(InProcess::new(endpoint, Outbox::new(8, 8, 0)));
    client.connect().unwrap();
    while client.poll().is_some() {}
    let mut graph = stereo_graph();
    graph.loops.truncate(2);
    let mut parent = graph.loops[0].clone();
    parent.id = id(120);
    parent.strip = None;
    let span = |target, start, end, ordinal| CompositeSpan {
        target: id(target),
        start,
        end,
        occurrence_start: start,
        ordinal,
        mode: LoopMode::Playing,
    };
    let old_length = 64;
    parent.composite = Some(CompositeView {
        kind: CompositeKind::Regular,
        version: 1,
        unit_frames: Q as u64,
        iterations: old_length,
        sample_rate: 48000,
        quantum: Q as u32,
        spans: vec![span(100, 0, old_length, 0), span(101, 0, 2, 1)].into(),
    });
    graph.loops.push(parent);
    if other_parent {
        let mut other = graph.loops[2].clone();
        other.id = id(121);
        other.composite.as_mut().unwrap().spans = vec![span(100, 0, old_length, 0)].into();
        graph.loops.push(other);
    }
    upload(&mut client, &mut renderer, &graph, 0);
    let transition = |slot, mode: TransitionMode| Command::Transition(slot, mode.into());
    for mode in [TransitionMode::Recording, TransitionMode::Stopped] {
        let children = [transition(100, mode), transition(101, mode)];
        client
            .immediate(Batch::new(1, 1, &children).unwrap())
            .unwrap();
        response(&mut client, &mut renderer);
        for _ in 0..8 {
            assert_eq!(step(&mut renderer), [0., 0.]);
        }
    }
    client
        .immediate(Batch::new(1, 1, &[transition(120, TransitionMode::Playing)]).unwrap())
        .unwrap();
    let started = response(&mut client, &mut renderer);
    for _ in 0..4 {
        step(&mut renderer);
    }
    assert_eq!(step(&mut renderer), [0.125, 0.125]);

    let edited = graph.loops[2].composite.as_mut().unwrap();
    edited.version = 2;
    edited.iterations = length;
    edited.spans = if drop_child {
        vec![span(101, 0, length, 1)]
    } else {
        vec![span(100, 4, 6, 0), span(101, 0, length, 1)]
    }
    .into();
    let switched = upload(&mut client, &mut renderer, &graph, 1);
    assert_eq!(switched, started + old_length * Q as u64);
    let first_sample = switched + Q as u64;
    assert_eq!(renderer.next_frame().0, first_sample);
    for iteration in 0..8 {
        let sample = if !drop_child && (4..6).contains(&iteration) {
            0.1875
        } else {
            0.0625
        };
        assert_eq!(
            step(&mut renderer),
            [sample, sample],
            "iteration {iteration}"
        );
    }

    client
        .immediate(Batch::new(2, 2, &[transition(100, TransitionMode::Playing)]).unwrap())
        .unwrap();
    response(&mut client, &mut renderer);
    let next_wrap = first_sample + length * Q as u64;
    while renderer.next_frame().0 <= next_wrap {
        assert_eq!(step(&mut renderer), [0.1875, 0.1875]);
    }
}

#[shoop_test(no_wasm = "actual Marlin simulation and reference renderer")]
fn continuous_wrap_edits_replace_the_old_ending_once() {
    let driver = DriverStatus {
        kind: DriverKind::Dummy,
        state: DriverRunState::Running,
        sample_rate: 48000,
        buffer_frames: Q as u32,
    };
    for (length, drop_child, other_parent) in [
        (24, false, false),
        (64, false, false),
        (24, true, false),
        (64, true, true),
    ] {
        let rust = shoop_rt_host_reference::prepare(Q as u32, driver, 1).unwrap();
        edit_at_continuous_wrap(
            rust.endpoint,
            rust.renderer,
            length,
            drop_child,
            other_parent,
        );
        let rtl = shoop_rt_host_marlin::prepare(Q as u32, driver, 1, Q).unwrap();
        edit_at_continuous_wrap(rtl.endpoint, rtl.renderer, length, drop_child, other_parent);
    }
}

#[shoop_test(no_wasm = "actual Marlin simulation and reference renderer")]
fn prepared_conversion_fences_an_old_generation_and_preserves_its_take() {
    let driver = DriverStatus {
        kind: DriverKind::Dummy,
        state: DriverRunState::Running,
        sample_rate: 48000,
        buffer_frames: Q as u32,
    };
    let rust = shoop_rt_host_reference::prepare(Q as u32, driver, 1).unwrap();
    composite_conversion(rust.endpoint, rust.renderer);
    let rtl = shoop_rt_host_marlin::prepare(Q as u32, driver, 1, Q).unwrap();
    composite_conversion(rtl.endpoint, rtl.renderer);
}

#[shoop_test(no_wasm = "actual Marlin simulation and reference renderer")]
fn prepared_composite_hosts_record_and_publish_stopped_and_restart_edits() {
    let driver = DriverStatus {
        kind: DriverKind::Dummy,
        state: DriverRunState::Running,
        sample_rate: 48000,
        buffer_frames: Q as u32,
    };
    let rust = shoop_rt_host_reference::prepare(Q as u32, driver, 1).unwrap();
    composite_round_trip(rust.endpoint, rust.renderer);
    let rtl = shoop_rt_host_marlin::prepare(Q as u32, driver, 1, Q).unwrap();
    composite_round_trip(rtl.endpoint, rtl.renderer);
}
