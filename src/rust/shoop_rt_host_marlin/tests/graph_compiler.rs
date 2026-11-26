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
    applied_frame(wait_response(c, r))
}

fn applied_frame(response: Response) -> u64 {
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
fn refuse_upload<E: Endpoint>(
    c: &mut ProtocolClient<InProcess<E>>,
    r: &mut impl Renderer,
    g: &Graph,
    revision: u64,
    code: Code,
) {
    let before = r.next_frame();
    for batch in shoop_rt_protocol::graph::upload(g, Q as u32, revision).unwrap() {
        let activation = batch.commands().contains(&Command::ActivateGraph);
        c.immediate(batch).unwrap();
        let result = wait_response(c, r);
        if activation {
            assert_eq!(result.header.code(), code);
            assert!(result.records().iter().all(|record| record.code() == code));
        } else {
            applied_frame(result);
        }
    }
    step(r);
    assert!(r.next_frame() > before);
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

fn monitor_nested_recording<E: Endpoint>(endpoint: E, mut renderer: impl Renderer) {
    use rt_fpga_compiler::shoop_graph::{CompositeKind, CompositeSpan, CompositeView, LoopMode};
    let mut client = ProtocolClient::new(InProcess::new(endpoint, Outbox::new(8, 8, 0)));
    client.connect().unwrap();
    while client.poll().is_some() {}
    let mut graph = stereo_graph();
    graph.strips[0].controls.automatic_monitoring = true;
    let span = |slot, start, end, mode, ordinal| CompositeSpan {
        target: id(slot),
        start,
        end,
        occurrence_start: start,
        ordinal,
        mode,
    };
    let mut child = graph.loops[0].clone();
    child.id = id(120);
    child.strip = None;
    child.composite = Some(CompositeView {
        kind: CompositeKind::Explicit,
        version: 1,
        unit_frames: Q as u64,
        iterations: 32,
        sample_rate: 48000,
        quantum: Q as u32,
        spans: vec![
            span(100, 4, 13, LoopMode::Recording, 0),
            span(101, 8, 20, LoopMode::Recording, 1),
        ]
        .into(),
    });
    let mut parent = child.clone();
    parent.id = id(121);
    let parent_plan = parent.composite.as_mut().unwrap();
    parent_plan.kind = CompositeKind::Regular;
    parent_plan.iterations = 64;
    parent_plan.spans = vec![span(120, 0, 32, LoopMode::Playing, 0)].into();
    graph.loops.extend([child, parent]);
    upload(&mut client, &mut renderer, &graph, 0);
    assert_eq!(step(&mut renderer), [0.; 2]);
    // This unrelated recorder must stay silent while the two nested
    // recorders share one input monitor.
    let commands = [
        Command::Transition(121, TransitionMode::Playing.into()),
        Command::Transition(102, TransitionMode::Recording.into()),
    ];
    client
        .immediate(Batch::new(1, 1, &commands).unwrap())
        .unwrap();
    let audible_start = response(&mut client, &mut renderer) + Q as u64;
    assert!(renderer.next_frame().0 <= audible_start + 4 * Q as u64);
    while renderer.next_frame().0 < audible_start + 32 * Q as u64 {
        let frame = renderer.next_frame().0;
        let monitored =
            (audible_start + 4 * Q as u64..audible_start + 20 * Q as u64).contains(&frame);
        let sample = if monitored { 0.25 } else { 0.0 };
        assert_eq!(step(&mut renderer), [sample; 2], "frame {frame}");
    }
    graph.strips[0].controls.input_monitoring = true;
    refuse_upload(
        &mut client,
        &mut renderer,
        &graph,
        1,
        Code::GraphCompositeUnsupported,
    );
    assert_eq!(step(&mut renderer), [0.; 2]);
}
#[shoop_test(no_wasm = "actual Marlin simulation and reference renderer")]
fn automatic_monitoring_follows_nested_recording_per_track() {
    let driver = DriverStatus {
        kind: DriverKind::Dummy,
        state: DriverRunState::Running,
        sample_rate: 48_000,
        buffer_frames: Q as u32,
    };
    let rust = shoop_rt_host_reference::prepare(Q as u32, driver, 1).unwrap();
    monitor_nested_recording(rust.endpoint, rust.renderer);
    let rtl = shoop_rt_host_marlin::prepare(Q as u32, driver, 1, Q).unwrap();
    monitor_nested_recording(rtl.endpoint, rtl.renderer);
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

fn driver() -> DriverStatus {
    DriverStatus {
        kind: DriverKind::Dummy,
        state: DriverRunState::Running,
        sample_rate: 48_000,
        buffer_frames: Q as u32,
    }
}

fn static_controls<E: Endpoint>(
    endpoint: E,
    mut renderer: impl Renderer,
    kind: rt_fpga_compiler::shoop_graph::CompositeKind,
) {
    use rt_fpga_compiler::shoop_graph::{CompositeKind, CompositeSpan, CompositeView, LoopMode};
    let mut client = ProtocolClient::new(InProcess::new(endpoint, Outbox::new(8, 8, 0)));
    client.connect().unwrap();
    while client.poll().is_some() {}
    let mut graph = stereo_graph();
    graph.loops.truncate(1);
    let mut parent = graph.loops[0].clone();
    parent.id = id(120);
    parent.strip = None;
    parent.composite = Some(CompositeView {
        kind,
        version: 1,
        unit_frames: Q as u64,
        iterations: 16,
        sample_rate: 48_000,
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
                start: 4,
                end: 16,
                occurrence_start: 4,
                ordinal: 1,
                mode: LoopMode::Playing,
            },
        ]
        .into(),
    });
    if kind == CompositeKind::Regular {
        let plan = parent.composite.as_mut().unwrap();
        plan.spans = vec![plan.spans[1]].into();
    }
    graph.loops.push(parent);
    let mut wrong_rate = graph.clone();
    wrong_rate.loops[1].composite.as_mut().unwrap().sample_rate = 44_100;
    refuse_upload(
        &mut client,
        &mut renderer,
        &wrong_rate,
        0,
        Code::InvalidValue,
    );
    upload(&mut client, &mut renderer, &graph, 0);
    if kind == CompositeKind::Regular {
        client
            .immediate(
                Batch::new(
                    1,
                    1,
                    &[Command::Transition(100, TransitionMode::Recording.into())],
                )
                .unwrap(),
            )
            .unwrap();
        response(&mut client, &mut renderer);
        for _ in 0..4 {
            step(&mut renderer);
        }
        client
            .immediate(
                Batch::new(
                    1,
                    1,
                    &[Command::Transition(100, TransitionMode::Stopped.into())],
                )
                .unwrap(),
            )
            .unwrap();
        response(&mut client, &mut renderer);
    }
    let transition = |mode: TransitionMode| Command::Transition(120, mode.into());
    let iteration = |position, seek| {
        Command::LoopIteration(
            LoopSlotId(SlotId::new(120, id(120).generation)),
            IterationTransition {
                iteration: position,
                seek,
                retrigger: false,
            },
        )
    };
    client
        .immediate(Batch::new(1, 1, &[transition(TransitionMode::Playing)]).unwrap())
        .unwrap();
    response(&mut client, &mut renderer);
    assert!((0..8).any(|_| step(&mut renderer) == [0.125; 2]));
    for (position, expected) in [(2, [0.; 2]), (4, [0.125; 2])] {
        client
            .immediate(Batch::new(1, 1, &[iteration(position, true)]).unwrap())
            .unwrap();
        response(&mut client, &mut renderer);
        assert_eq!(step(&mut renderer), expected);
    }
    client
        .immediate(Batch::new(1, 1, &[iteration(16, true)]).unwrap())
        .unwrap();
    assert_eq!(
        wait_response(&mut client, &mut renderer).records()[0].code(),
        Code::InvalidValue
    );
    let Command::LoopIteration(_, request) = iteration(4, true) else {
        unreachable!()
    };
    client
        .immediate(
            Batch::new(
                1,
                1,
                &[Command::LoopIteration(
                    LoopSlotId(SlotId::new(120, id(120).generation + 1)),
                    request,
                )],
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        wait_response(&mut client, &mut renderer).records()[0].code(),
        Code::TargetUnavailable
    );
    client
        .immediate(Batch::new(1, 1, &[transition(TransitionMode::Stopped)]).unwrap())
        .unwrap();
    response(&mut client, &mut renderer);
    assert_eq!(step(&mut renderer), [0.; 2]);
    for change in 0..4 {
        let mut candidate = graph.clone();
        match change {
            0 => candidate.loops[1].composite.as_mut().unwrap().version += 1,
            1 => candidate.loops[0].gain = 0.,
            2 => candidate.loops.truncate(1),
            _ => candidate.buses[0].muted = true,
        }
        refuse_upload(
            &mut client,
            &mut renderer,
            &candidate,
            1,
            Code::GraphCompositeUnsupported,
        );
        assert_eq!(step(&mut renderer), [0.; 2]);
        client
            .immediate(Batch::new(1, 1, &[iteration(4, false)]).unwrap())
            .unwrap();
        response(&mut client, &mut renderer);
        assert_eq!(step(&mut renderer), [0.125; 2]);
        client
            .immediate(Batch::new(1, 1, &[transition(TransitionMode::Stopped)]).unwrap())
            .unwrap();
        response(&mut client, &mut renderer);
    }
    client
        .immediate(Batch::new(1, 1, &[iteration(4, false)]).unwrap())
        .unwrap();
    response(&mut client, &mut renderer);
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
    assert_eq!(
        wait_response(&mut client, &mut renderer).records()[0].code(),
        Code::Scheduled
    );
    for _ in 0..20 {
        step(&mut renderer);
    }
    for _ in 0..4 {
        assert_eq!(step(&mut renderer), [0.; 2]);
    }
    client
        .immediate(
            Batch::new(
                1,
                1,
                &[Command::LoopRepeatSync(
                    LoopSlotId(SlotId::new(120, id(120).generation)),
                    true,
                )],
            )
            .unwrap(),
        )
        .unwrap();
    response(&mut client, &mut renderer);
    client
        .immediate(
            Batch::new(
                1,
                1,
                &[Command::LoopRepeatSync(
                    LoopSlotId(SlotId::new(120, id(120).generation)),
                    false,
                )],
            )
            .unwrap(),
        )
        .unwrap();
    response(&mut client, &mut renderer);
}
#[shoop_test(no_wasm = "actual Marlin simulation and reference renderer")]
fn static_composite_controls_survive_refused_live_edits() {
    use rt_fpga_compiler::shoop_graph::CompositeKind;
    for kind in [CompositeKind::Regular, CompositeKind::Explicit] {
        let rust = shoop_rt_host_reference::prepare(Q as u32, driver(), 1).unwrap();
        static_controls(rust.endpoint, rust.renderer, kind);
        let rtl = shoop_rt_host_marlin::prepare(Q as u32, driver(), 1, Q).unwrap();
        static_controls(rtl.endpoint, rtl.renderer, kind);
    }
}
fn refuse_introduction<E: Endpoint>(endpoint: E, mut renderer: impl Renderer) {
    use rt_fpga_compiler::shoop_graph::{CompositeKind, CompositeSpan, CompositeView, LoopMode};
    let mut client = ProtocolClient::new(InProcess::new(endpoint, Outbox::new(8, 8, 0)));
    client.connect().unwrap();
    while client.poll().is_some() {}
    let mut graph = stereo_graph();
    graph.loops.truncate(1);
    upload(&mut client, &mut renderer, &graph, 0);
    client
        .immediate(
            Batch::new(
                1,
                1,
                &[Command::Transition(100, TransitionMode::Recording.into())],
            )
            .unwrap(),
        )
        .unwrap();
    response(&mut client, &mut renderer);
    for _ in 0..4 {
        step(&mut renderer);
    }
    client
        .immediate(
            Batch::new(
                1,
                1,
                &[Command::Transition(100, TransitionMode::Playing.into())],
            )
            .unwrap(),
        )
        .unwrap();
    response(&mut client, &mut renderer);
    assert_eq!(step(&mut renderer), [0.125; 2]);
    let mut candidate = graph.clone();
    let mut parent = graph.loops[0].clone();
    parent.id = id(120);
    parent.strip = None;
    parent.composite = Some(CompositeView {
        kind: CompositeKind::Regular,
        version: 1,
        unit_frames: Q as u64,
        iterations: 16,
        sample_rate: 48_000,
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
    candidate.loops.push(parent);
    refuse_upload(
        &mut client,
        &mut renderer,
        &candidate,
        1,
        Code::GraphCompositeUnsupported,
    );
    for _ in 0..4 {
        assert_eq!(step(&mut renderer), [0.125; 2]);
    }
    graph.loops[0].gain = 0.25;
    upload(&mut client, &mut renderer, &graph, 1);
    client
        .immediate(Batch::new(2, 2, &[Command::LoopGain(100, 0.25)]).unwrap())
        .unwrap();
    response(&mut client, &mut renderer);
    assert_eq!(step(&mut renderer), [0.0625; 2]);
}
#[shoop_test(no_wasm = "actual Marlin simulation and reference renderer")]
fn refused_composite_introduction_preserves_playback_and_primitive_switching() {
    let rust = shoop_rt_host_reference::prepare(Q as u32, driver(), 1).unwrap();
    refuse_introduction(rust.endpoint, rust.renderer);
    let rtl = shoop_rt_host_marlin::prepare(Q as u32, driver(), 1, Q).unwrap();
    refuse_introduction(rtl.endpoint, rtl.renderer);
}

fn shared_reentry<E: Endpoint>(endpoint: E, mut renderer: impl Renderer) {
    use rt_fpga_compiler::shoop_graph::{CompositeKind, CompositeSpan, CompositeView, LoopMode};
    let mut client = ProtocolClient::new(InProcess::new(endpoint, Outbox::new(8, 8, 0)));
    client.connect().unwrap();
    while client.poll().is_some() {}
    let mut graph = stereo_graph();
    graph.loops.truncate(1);
    let span = |target, start, end, mode| CompositeSpan {
        target: id(target),
        start,
        end,
        occurrence_start: start,
        ordinal: 0,
        mode,
    };
    let mut child = graph.loops[0].clone();
    child.id = id(121);
    child.strip = None;
    child.composite = Some(CompositeView {
        kind: CompositeKind::Explicit,
        version: 1,
        unit_frames: Q as u64,
        iterations: 3,
        sample_rate: 48_000,
        quantum: Q as u32,
        spans: vec![
            span(100, 0, 2, LoopMode::Playing),
            span(100, 2, 3, LoopMode::Stopped),
        ]
        .into(),
    });
    let mut first = child.clone();
    first.id = id(120);
    let plan = first.composite.as_mut().unwrap();
    plan.kind = CompositeKind::Regular;
    plan.iterations = 8;
    plan.spans = vec![span(121, 0, 3, LoopMode::Playing)].into();
    let mut second = first.clone();
    second.id = id(122);
    second.composite.as_mut().unwrap().spans = vec![span(121, 4, 7, LoopMode::Playing)].into();
    graph.loops.extend([child, first, second]);
    upload(&mut client, &mut renderer, &graph, 0);
    client
        .immediate(
            Batch::new(
                1,
                1,
                &[Command::Transition(100, TransitionMode::Recording.into())],
            )
            .unwrap(),
        )
        .unwrap();
    response(&mut client, &mut renderer);
    for _ in 0..4 {
        step(&mut renderer);
    }
    client
        .immediate(
            Batch::new(
                1,
                1,
                &[Command::Transition(100, TransitionMode::Stopped.into())],
            )
            .unwrap(),
        )
        .unwrap();
    response(&mut client, &mut renderer);
    client
        .immediate(
            Batch::new(
                1,
                1,
                &[
                    Command::Transition(120, TransitionMode::Playing.into()),
                    Command::Transition(122, TransitionMode::Playing.into()),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    let audible_start = response(&mut client, &mut renderer) + Q as u64;
    assert!(renderer.next_frame().0 <= audible_start + 4 * Q as u64);
    while renderer.next_frame().0 < audible_start + 24 * Q as u64 {
        let frame = renderer.next_frame().0;
        let playing = frame >= audible_start && (frame - audible_start) / (Q as u64) % 4 < 2;
        assert_eq!(
            step(&mut renderer),
            [if playing { 0.125 } else { 0. }; 2],
            "frame {frame}"
        );
    }
}

#[shoop_test(no_wasm = "actual Marlin simulation and reference renderer")]
fn static_shared_nested_one_shots_reenter_without_losing_their_take() {
    let rust = shoop_rt_host_reference::prepare(Q as u32, driver(), 1).unwrap();
    shared_reentry(rust.endpoint, rust.renderer);
    let rtl = shoop_rt_host_marlin::prepare(Q as u32, driver(), 1, Q).unwrap();
    shared_reentry(rtl.endpoint, rtl.renderer);
}
