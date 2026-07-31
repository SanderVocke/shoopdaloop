use shoop_engine::{
    compile_composite_plan, AcceptedTimelineControl, BoundaryTargetAction,
    CompositeBoundaryTimeline, CompositeEntry, CompositePlanDescriptor, CompositePlanLimits,
    CompositeSection, CompositeTimeline, CompositeTimelineLimits, CompositeTimelineNode,
    LoopIdentity, LoopMode, LoopTargetCatalog, LoopTargetKind, LoopTargetMetadata, Session,
};
use std::time::{Duration, Instant};

const CALLBACK_FRAMES: usize = 64;

fn make_session(n_composites: usize, n_targets: usize) -> (Session, Vec<LoopIdentity>) {
    let mut session = Session::default();
    let sync = session.create_loop();
    session
        .loop_mut(sync)
        .unwrap()
        .set_length(CALLBACK_FRAMES as u32);
    session.set_loop_mode(sync, LoopMode::Playing).unwrap();
    let sync_identity = session.loop_identity(sync).unwrap();

    let mut targets = Vec::with_capacity(n_targets);
    for _ in 0..n_targets {
        let index = session.create_loop();
        session
            .loop_mut(index)
            .unwrap()
            .set_length(CALLBACK_FRAMES as u32);
        targets.push(session.loop_identity(index).unwrap());
    }

    let sources: Vec<_> = (0..n_composites)
        .map(|index| LoopIdentity {
            slot: 10_000 + index as u32,
            generation: 1,
            kind: LoopTargetKind::Composite,
        })
        .collect();
    let mut metadata = Vec::with_capacity(1 + targets.len() + sources.len());
    metadata.push(LoopTargetMetadata {
        identity: sync_identity,
        length_samples: CALLBACK_FRAMES as u64,
    });
    metadata.extend(targets.iter().copied().map(|identity| LoopTargetMetadata {
        identity,
        length_samples: CALLBACK_FRAMES as u64,
    }));
    metadata.extend(sources.iter().copied().map(|identity| LoopTargetMetadata {
        identity,
        length_samples: 0,
    }));
    let catalog = LoopTargetCatalog::new(metadata).unwrap();

    let entries: Vec<_> = targets
        .iter()
        .copied()
        .map(|target| CompositeEntry {
            target,
            delay: 0,
            n_cycles: Some(1),
            mode: Some(LoopMode::Playing),
        })
        .collect();
    let mut nodes = Vec::with_capacity(n_composites);
    for source in sources.iter().copied() {
        let plan = compile_composite_plan(
            &CompositePlanDescriptor {
                source,
                sync_length: CALLBACK_FRAMES as u64,
                timelines: vec![CompositeTimeline {
                    sections: vec![CompositeSection {
                        entries: entries.clone(),
                    }],
                }],
            },
            &catalog,
            &[],
            CompositePlanLimits::default(),
        )
        .unwrap();
        nodes.push(CompositeTimelineNode {
            plan,
            sync_source: sync_identity,
        });
    }
    let timeline =
        CompositeBoundaryTimeline::new(nodes, CompositeTimelineLimits::default()).unwrap();
    session.install_composite_timeline(timeline).unwrap();
    session.apply_graph_changes().unwrap();
    (session, sources)
}

fn run_case(n_composites: usize, n_targets: usize, callbacks: usize) -> Duration {
    let (mut session, sources) = make_session(n_composites, n_targets);
    let mut sequence = 0u64;
    let mut run_callback = |session: &mut Session| {
        let at_sample = session.composite_timeline().sample_clock();
        for source in sources.iter().copied() {
            session
                .composite_timeline_mut()
                .queue_control(AcceptedTimelineControl {
                    at_sample,
                    target: source,
                    action: BoundaryTargetAction::SetMode {
                        mode: LoopMode::Playing,
                        offset_samples: 0,
                        retrigger: true,
                    },
                    acceptance_sequence: sequence,
                })
                .unwrap();
            sequence += 1;
        }
        session.process(CALLBACK_FRAMES);
    };

    run_callback(&mut session);
    let started = Instant::now();
    for _ in 0..callbacks {
        run_callback(&mut session);
    }
    started.elapsed()
}

fn report(name: &str, n_composites: usize, n_targets: usize, callbacks: usize) {
    let elapsed = run_case(n_composites, n_targets, callbacks);
    let per_callback = elapsed.as_secs_f64() * 1_000_000.0 / callbacks as f64;
    println!(
        "{name}: {n_composites} composites x {n_targets} targets, {callbacks} callbacks: \
         {:.3} us/callback ({:.3} ms total)",
        per_callback,
        elapsed.as_secs_f64() * 1_000.0
    );
}

fn main() {
    report("ordinary", 1, 4, 20_000);
    report("maximum", 64, 64, 500);
}
