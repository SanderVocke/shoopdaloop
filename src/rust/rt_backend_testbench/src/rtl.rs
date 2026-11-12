//! Session fixture assembly around the shared production-order hardware lowerer.
use crate::{production, spec::*, validation::Checked};
pub use rt_fpga_adapter::image::*;
use std::collections::BTreeMap;
pub fn compile(scenario: &Scenario) -> Checked<Image> {
    let production::Prepared { ordered, .. } = production::prepare::<rt_runtime::Q824>(scenario)?;
    let slots = scenario
        .loops
        .iter()
        .map(|l| l.id.slot + 1)
        .max()
        .unwrap_or(1);
    let requests = u32::from(scenario.request_capacity);
    let topology = production::topology(scenario, slots as usize, requests as usize)?;
    let components = topology
        .components()
        .iter()
        .map(|c| {
            c.iter()
                .map(|id| Identity {
                    slot: id.slot(),
                    generation: id.generation(),
                })
                .collect()
        })
        .collect();
    let transition = TransitionImage {
        description: Description {
            quantum: scenario.quantum,
            loops: topology
                .nodes()
                .iter()
                .map(|n| {
                    let source = scenario
                        .loops
                        .iter()
                        .find(|l| production::loop_id(l.id).ok() == Some(n.id))
                        .unwrap();
                    LoopSpec {
                        id: source.id,
                        kind: "primitive".into(),
                        length: n.length,
                        sync_source: source.sync_source,
                        repeat_sync: source.repeat_sync,
                        timeline: None,
                    }
                })
                .collect(),
        },
        limits: Limits {
            loop_slots: slots,
            pending_requests: requests,
            events: slots * 2 + requests,
        },
        components,
    };
    let mut media = MediaImage {
        chunk_frames: scenario.chunk_frames,
        samples: Vec::new(),
        free_chunks: Vec::new(),
        views: Vec::new(),
    };
    let mut banks = BTreeMap::new();
    for m in &scenario.media {
        for (channel, samples) in m.channels.iter().enumerate() {
            let first = media.samples.len() / scenario.chunk_frames as usize;
            media.samples.extend(samples);
            banks.insert((m.name.as_str(), channel), first as u32);
        }
    }
    let mut loop_views = Vec::new();
    for l in &scenario.loops {
        let mut views = Vec::new();
        for channel in 0..l.channels as usize {
            views.push(media.views.len() as u32);
            let (chunks, frames) = if let Some(c) = &l.content {
                let first = banks[&(c.media.as_str(), channel)] + c.start_chunks as u32;
                (
                    (0..c.length_chunks).map(|i| first + i as u32).collect(),
                    c.length_chunks * u64::from(scenario.chunk_frames),
                )
            } else {
                (Vec::new(), 0)
            };
            media.views.push(View { chunks, frames });
        }
        loop_views.push(LoopViews { id: l.id, views });
    }
    let referenced: std::collections::BTreeSet<_> = media
        .views
        .iter()
        .flat_map(|v| v.chunks.iter().copied())
        .collect();
    media.free_chunks = (0..media.samples.len() as u32 / scenario.chunk_frames)
        .filter(|id| !referenced.contains(id))
        .collect();
    let nodes = scenario
        .nodes
        .iter()
        .map(|n| {
            crate::validation::Checked::Ok(rt_fpga_adapter::compiler::NodeDescriptor {
                id: n.id,
                operation: n.operation,
                channels: match n.operation {
                    NodeKind::Gain { channels, .. } => channels,
                    NodeKind::Playback { target } | NodeKind::Capture { target } => {
                        scenario
                            .loops
                            .iter()
                            .find(|l| l.id == target)
                            .ok_or("unknown loop")?
                            .channels
                    }
                },
            })
        })
        .collect::<Checked<Vec<_>>>()?;
    rt_fpga_adapter::compiler::lower(
        &ordered,
        &nodes,
        Image {
            format_version: FORMAT_VERSION,
            name: scenario.name.clone(),
            transition,
            program: Program {
                instructions: Vec::new(),
                quantum: scenario.quantum,
                chunk_frames: scenario.chunk_frames,
                jobs_per_quantum: 0,
            },
            initial_state: Memory { memory: Vec::new() },
            buffers: 0,
            input_ports: scenario.input_ports,
            output_ports: scenario.output_ports,
            media,
            loop_views,
            gains: Vec::new(),
            node_order: Vec::new(),
        },
    )
}
