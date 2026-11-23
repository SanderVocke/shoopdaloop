use rt_fpga_compiler::shoop_graph::{
    ChannelLayout, CompositeKind, CompositeSpan, CompositeView, Graph, Id, LoopMode, LoopView,
};

fn id(slot: u32) -> Id {
    Id {
        slot,
        generation: 1,
    }
}

pub fn timing_loop(slot: u32, iterations: u64, children: &[(u32, u64, u64)]) -> LoopView<'static> {
    LoopView {
        id: id(slot),
        strip: None,
        layout: ChannelLayout::Direct {
            audio: 0,
            midi: false,
        },
        gain: 1.,
        balance: 0.,
        boundary_smoothing_frames: 0,
        repeat_sync: false,
        sync_source: None,
        composite: (!children.is_empty()).then(|| CompositeView {
            kind: CompositeKind::Regular,
            version: 1,
            unit_frames: 16,
            iterations,
            sample_rate: 48000,
            quantum: 16,
            spans: children
                .iter()
                .enumerate()
                .map(|(ordinal, &(child, start, end))| CompositeSpan {
                    target: id(child),
                    start,
                    end,
                    occurrence_start: start,
                    ordinal: ordinal as u32,
                    mode: LoopMode::Playing,
                })
                .collect::<Vec<_>>()
                .into(),
        }),
    }
}

pub fn arbitration_graph() -> Graph {
    let mut graph = Graph::default();
    graph.loops = vec![
        timing_loop(0, 0, &[]),
        timing_loop(1, 4, &[(4, 0, 4), (3, 0, 4)]),
        timing_loop(2, 3, &[(4, 0, 1), (4, 1, 3)]),
        timing_loop(3, 2, &[(5, 0, 2)]),
        timing_loop(4, 0, &[]),
        timing_loop(5, 0, &[]),
        timing_loop(6, 0, &[]),
    ];
    graph.loops[2].composite.as_mut().unwrap().kind = CompositeKind::Explicit;
    graph.loops[2].composite.as_mut().unwrap().spans.to_mut()[0].mode = LoopMode::Stopped;
    graph.loops[3].composite.as_mut().unwrap().unit_frames = 32;
    graph.loops[6].composite = Some(CompositeView {
        kind: CompositeKind::Regular,
        version: 1,
        unit_frames: 16,
        iterations: 0,
        sample_rate: 48000,
        quantum: 16,
        spans: (&[][..]).into(),
    });
    graph.loops[4].sync_source = Some(id(0));
    graph
}
