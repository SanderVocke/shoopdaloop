//! Turns a description of ports, loops and channels into scheduler nodes.
//!
//! `weak_ptr`s to its neighbours. Here the whole topology is stated declaratively
//! and lowered to [`NodeSpec`]s in one pass, so the edge rules live in one place
//! and can be read end to end.
//!
//!
//! - a port has two nodes: `prepare` (acquire buffers) and
//!   `process_and_internal_connections` (apply the signal path, then pass through
//!   to internally connected ports)
//! - a channel has two: `prepare_buffers` (point at its ports' buffers) and
//!   `process` (settle after the loop has run)
//! - a loop has one: `process`

use crate::graph::{NodeIdx, NodeSpec};

/// Index into the described ports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PortIdx(pub usize);
/// Index into the described processors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProcessorIdx(pub usize);
/// Index into the described loops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LoopIdx(pub usize);
/// Index into the described channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChannelIdx(pub usize);

#[derive(Debug, Clone)]
pub struct PortDesc {
    pub name: String,
    /// Ports this one passes its output through to.
    pub internal_connections: Vec<PortIdx>,
}

#[derive(Debug, Clone)]
pub struct ProcessorDesc {
    pub name: String,
    pub input_ports: Vec<PortIdx>,
    pub output_ports: Vec<PortIdx>,
}

#[derive(Debug, Clone, Default)]
pub struct LoopDesc {
    /// Loops that must be processed in the same step, so they stay in sync.
    pub co_process_with: Vec<LoopIdx>,
}

#[derive(Debug, Clone)]
pub struct ChannelDesc {
    pub loop_idx: LoopIdx,
    pub input_port: Option<PortIdx>,
    pub output_port: Option<PortIdx>,
}

/// What each entity's nodes are called, for reading a schedule back.
#[derive(Debug, Clone, Default)]
pub struct NodeMap {
    pub port_prepare: Vec<NodeIdx>,
    pub port_process: Vec<NodeIdx>,
    pub processor_process: Vec<NodeIdx>,
    pub loop_process: Vec<NodeIdx>,
    pub channel_prepare: Vec<NodeIdx>,
    pub channel_process: Vec<NodeIdx>,
}

#[derive(Debug, Clone, Default)]
pub struct GraphDesc {
    pub ports: Vec<PortDesc>,
    pub processors: Vec<ProcessorDesc>,
    pub loops: Vec<LoopDesc>,
    pub channels: Vec<ChannelDesc>,
}

impl GraphDesc {
    /// Lowers the description to scheduler nodes.
    ///
    /// unions both directions, so the result is identical either way.
    #[tracing::instrument(name = "engine.graph.build_topology", skip_all)]
    pub fn build(&self) -> (Vec<NodeSpec>, NodeMap) {
        let mut map = NodeMap::default();
        let mut next = 0usize;
        let mut alloc = || {
            let i = NodeIdx(next);
            next += 1;
            i
        };

        // Allocate in a fixed order so node indices are reproducible.
        for _ in &self.ports {
            map.port_prepare.push(alloc());
            map.port_process.push(alloc());
        }
        for _ in &self.processors {
            map.processor_process.push(alloc());
        }
        for _ in &self.loops {
            map.loop_process.push(alloc());
        }
        for _ in &self.channels {
            map.channel_prepare.push(alloc());
            map.channel_process.push(alloc());
        }

        let mut specs = vec![NodeSpec::default(); next];

        for (i, port) in self.ports.iter().enumerate() {
            let prepare = map.port_prepare[i];
            let process = map.port_process[i];
            specs[prepare.0].name = format!("{}::prepare", port.name);
            specs[process.0].name = format!("{}::process_and_internal_connections", port.name);

            // Our own buffers must exist before we process them.
            specs[process.0].incoming.push(prepare);
            for target in &port.internal_connections {
                // A pass-through target's buffers must be ready before we write
                // into them, and its processing must follow ours.
                specs[process.0].incoming.push(map.port_prepare[target.0]);
                specs[process.0].outgoing.push(map.port_process[target.0]);
            }
        }

        for (i, processor) in self.processors.iter().enumerate() {
            let node = map.processor_process[i];
            specs[node.0].name = format!("processor::{}", processor.name);
            for input in &processor.input_ports {
                specs[node.0].incoming.push(map.port_process[input.0]);
            }
            for output in &processor.output_ports {
                specs[node.0].incoming.push(map.port_prepare[output.0]);
                specs[node.0].outgoing.push(map.port_process[output.0]);
            }
        }

        for (i, l) in self.loops.iter().enumerate() {
            let node = map.loop_process[i];
            specs[node.0].name = "loop::process".to_string();
            for other in &l.co_process_with {
                specs[node.0].co_process.push(map.loop_process[other.0]);
            }
        }

        for (i, c) in self.channels.iter().enumerate() {
            let prepare = map.channel_prepare[i];
            let process = map.channel_process[i];
            specs[prepare.0].name = "channel::prepare_buffers".to_string();
            specs[process.0].name = "channel::process".to_string();
            let loop_node = map.loop_process[c.loop_idx.0];

            // Buffers of both ports must exist before we point at them.
            for p in [c.input_port, c.output_port].into_iter().flatten() {
                specs[prepare.0].incoming.push(map.port_prepare[p.0]);
            }
            // The loop reads what we prepared.
            specs[prepare.0].outgoing.push(loop_node);

            // We settle after our own preparation and after the loop ran.
            specs[process.0].incoming.push(prepare);
            specs[process.0].incoming.push(loop_node);
            if let Some(input) = c.input_port {
                // Recording needs the input port's processed signal.
                specs[process.0].incoming.push(map.port_process[input.0]);
            }
            if let Some(output) = c.output_port {
                // Playback must land before the output port passes it on.
                specs[process.0].outgoing.push(map.port_process[output.0]);
            }
        }

        (specs, map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::processing_order;
    use assert2::{check, let_assert};

    fn port(name: &str, connections: &[usize]) -> PortDesc {
        PortDesc {
            name: name.to_string(),
            internal_connections: connections.iter().map(|&i| PortIdx(i)).collect(),
        }
    }

    fn processor(name: &str, inputs: &[usize], outputs: &[usize]) -> ProcessorDesc {
        ProcessorDesc {
            name: name.to_string(),
            input_ports: inputs.iter().copied().map(PortIdx).collect(),
            output_ports: outputs.iter().copied().map(PortIdx).collect(),
        }
    }

    fn names(specs: &[NodeSpec], schedule: &[Vec<NodeIdx>]) -> Vec<Vec<String>> {
        schedule
            .iter()
            .map(|step| {
                let mut n: Vec<String> = step.iter().map(|i| specs[i.0].name.clone()).collect();
                n.sort();
                n
            })
            .collect()
    }

    // The three cases below are the expected schedules asserted in
    // describing the same topology rather than hand-stating edges.

    #[tracy_nextest_capture::tracy_capture_test]
    fn two_ports() {
        let desc = GraphDesc {
            ports: vec![port("p1", &[1]), port("p2", &[])],
            ..Default::default()
        };
        let (specs, _) = desc.build();
        let_assert!(Ok(schedule) = processing_order(&specs));
        check!(
            names(&specs, &schedule)
                == vec![
                    vec!["p1::prepare".to_string()],
                    vec!["p2::prepare".to_string()],
                    vec!["p1::process_and_internal_connections".to_string()],
                    vec!["p2::process_and_internal_connections".to_string()],
                ]
        );
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn direct_loop() {
        let desc = GraphDesc {
            ports: vec![port("p1", &[1]), port("p2", &[])],
            processors: vec![],
            loops: vec![LoopDesc::default()],
            channels: vec![ChannelDesc {
                loop_idx: LoopIdx(0),
                input_port: Some(PortIdx(0)),
                output_port: Some(PortIdx(1)),
            }],
        };
        let (specs, _) = desc.build();
        let_assert!(Ok(schedule) = processing_order(&specs));
        check!(
            names(&specs, &schedule)
                == vec![
                    vec!["p1::prepare".to_string()],
                    vec!["p2::prepare".to_string()],
                    vec!["channel::prepare_buffers".to_string()],
                    vec!["p1::process_and_internal_connections".to_string()],
                    vec!["loop::process".to_string()],
                    vec!["channel::process".to_string()],
                    vec!["p2::process_and_internal_connections".to_string()],
                ]
        );
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn two_direct_loops_co_processed() {
        let desc = GraphDesc {
            ports: vec![port("p1", &[1]), port("p2", &[])],
            processors: vec![],
            loops: vec![
                LoopDesc {
                    co_process_with: vec![LoopIdx(0), LoopIdx(1)],
                },
                LoopDesc {
                    co_process_with: vec![LoopIdx(0), LoopIdx(1)],
                },
            ],
            channels: vec![
                ChannelDesc {
                    loop_idx: LoopIdx(0),
                    input_port: Some(PortIdx(0)),
                    output_port: Some(PortIdx(1)),
                },
                ChannelDesc {
                    loop_idx: LoopIdx(1),
                    input_port: Some(PortIdx(0)),
                    output_port: Some(PortIdx(1)),
                },
            ],
        };
        let (specs, _) = desc.build();
        let_assert!(Ok(schedule) = processing_order(&specs));
        check!(
            names(&specs, &schedule)
                == vec![
                    vec!["p1::prepare".to_string()],
                    vec!["p2::prepare".to_string()],
                    vec!["channel::prepare_buffers".to_string()],
                    vec!["channel::prepare_buffers".to_string()],
                    vec!["p1::process_and_internal_connections".to_string()],
                    vec!["loop::process".to_string(), "loop::process".to_string()],
                    vec!["channel::process".to_string()],
                    vec!["channel::process".to_string()],
                    vec!["p2::process_and_internal_connections".to_string()],
                ]
        );
    }

    // --- edge sets, asserted directly ---
    //
    // Some declared edges are redundant for scheduling: the ordering they impose
    // is already implied by other edges in any realistic topology, so no schedule
    // them would leave the graph relying on incidental structure, so they are
    // pinned here rather than through a schedule.

    /// Sorted incoming edge targets of a node, by name.
    fn incoming_names(specs: &[NodeSpec], node: NodeIdx) -> Vec<String> {
        let mut n: Vec<String> = specs[node.0]
            .incoming
            .iter()
            .map(|i| specs[i.0].name.clone())
            .collect();
        n.sort();
        n
    }

    fn outgoing_names(specs: &[NodeSpec], node: NodeIdx) -> Vec<String> {
        let mut n: Vec<String> = specs[node.0]
            .outgoing
            .iter()
            .map(|i| specs[i.0].name.clone())
            .collect();
        n.sort();
        n
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn a_port_declares_its_own_prepare_as_a_dependency() {
        let desc = GraphDesc {
            ports: vec![port("solo", &[])],
            ..Default::default()
        };
        let (specs, map) = desc.build();
        check!(incoming_names(&specs, map.port_process[0]) == vec!["solo::prepare".to_string()]);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn a_passthrough_source_waits_for_its_targets_buffers() {
        let desc = GraphDesc {
            ports: vec![port("p1", &[1]), port("p2", &[])],
            ..Default::default()
        };
        let (specs, map) = desc.build();
        // p1 cannot write into p2 until p2 has prepared its buffer.
        check!(
            incoming_names(&specs, map.port_process[0])
                == vec!["p1::prepare".to_string(), "p2::prepare".to_string()]
        );
        check!(
            outgoing_names(&specs, map.port_process[0])
                == vec!["p2::process_and_internal_connections".to_string()]
        );
        // p2 declares nothing of its own beyond its prepare.
        check!(incoming_names(&specs, map.port_process[1]) == vec!["p2::prepare".to_string()]);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn a_channel_declares_its_full_edge_set() {
        let desc = GraphDesc {
            ports: vec![port("in", &[]), port("out", &[])],
            processors: vec![],
            loops: vec![LoopDesc::default()],
            channels: vec![ChannelDesc {
                loop_idx: LoopIdx(0),
                input_port: Some(PortIdx(0)),
                output_port: Some(PortIdx(1)),
            }],
        };
        let (specs, map) = desc.build();

        // Prepare waits for both ports' buffers and gates the loop.
        check!(
            incoming_names(&specs, map.channel_prepare[0])
                == vec!["in::prepare".to_string(), "out::prepare".to_string()]
        );
        check!(outgoing_names(&specs, map.channel_prepare[0]) == vec!["loop::process".to_string()]);

        // Process waits for its own prepare, the loop, and the input port's
        // processed signal; and precedes the output port's processing.
        check!(
            incoming_names(&specs, map.channel_process[0])
                == vec![
                    "channel::prepare_buffers".to_string(),
                    "in::process_and_internal_connections".to_string(),
                    "loop::process".to_string(),
                ]
        );
        check!(
            outgoing_names(&specs, map.channel_process[0])
                == vec!["out::process_and_internal_connections".to_string()]
        );
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn a_processor_orders_dry_playback_before_wet_recording() {
        let desc = GraphDesc {
            ports: vec![port("dry-send", &[]), port("wet-return", &[])],
            processors: vec![processor("fx", &[0], &[1])],
            loops: vec![LoopDesc::default()],
            channels: vec![
                ChannelDesc {
                    loop_idx: LoopIdx(0),
                    input_port: None,
                    output_port: Some(PortIdx(0)),
                },
                ChannelDesc {
                    loop_idx: LoopIdx(0),
                    input_port: Some(PortIdx(1)),
                    output_port: None,
                },
            ],
        };
        let (specs, map) = desc.build();
        let_assert!(Ok(schedule) = processing_order(&specs));
        let pos = |node: NodeIdx| {
            schedule
                .iter()
                .position(|step| step.contains(&node))
                .unwrap()
        };
        check!(pos(map.channel_process[0]) < pos(map.port_process[0]));
        check!(pos(map.port_process[0]) < pos(map.processor_process[0]));
        check!(pos(map.processor_process[0]) < pos(map.port_process[1]));
        check!(pos(map.port_process[1]) < pos(map.channel_process[1]));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn a_processor_waits_for_all_inputs_and_prepares_all_outputs() {
        let desc = GraphDesc {
            ports: vec![
                port("audio-in", &[]),
                port("midi-in", &[]),
                port("left-out", &[]),
                port("right-out", &[]),
            ],
            processors: vec![processor("fx", &[0, 1], &[2, 3])],
            ..Default::default()
        };
        let (specs, map) = desc.build();
        check!(
            incoming_names(&specs, map.processor_process[0])
                == vec![
                    "audio-in::process_and_internal_connections".to_string(),
                    "left-out::prepare".to_string(),
                    "midi-in::process_and_internal_connections".to_string(),
                    "right-out::prepare".to_string(),
                ]
        );
        check!(
            outgoing_names(&specs, map.processor_process[0])
                == vec![
                    "left-out::process_and_internal_connections".to_string(),
                    "right-out::process_and_internal_connections".to_string(),
                ]
        );
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn a_loop_declares_no_edges_of_its_own() {
        let desc = GraphDesc {
            ports: vec![],
            processors: vec![],
            loops: vec![LoopDesc::default()],
            channels: vec![],
        };
        let (specs, map) = desc.build();
        // All ordering around a loop comes from its channels.
        check!(specs[map.loop_process[0].0].incoming.is_empty());
        check!(specs[map.loop_process[0].0].outgoing.is_empty());
    }

    // --- structural properties ---

    #[tracy_nextest_capture::tracy_capture_test]
    fn an_empty_description_yields_no_nodes() {
        let (specs, map) = GraphDesc::default().build();
        check!(specs.is_empty());
        check!(map.port_prepare.is_empty());
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn node_map_covers_every_entity() {
        let desc = GraphDesc {
            ports: vec![port("a", &[]), port("b", &[])],
            processors: vec![],
            loops: vec![LoopDesc::default()],
            channels: vec![ChannelDesc {
                loop_idx: LoopIdx(0),
                input_port: None,
                output_port: None,
            }],
        };
        let (specs, map) = desc.build();
        check!(map.port_prepare.len() == 2);
        check!(map.port_process.len() == 2);
        check!(map.loop_process.len() == 1);
        check!(map.channel_prepare.len() == 1);
        check!(map.channel_process.len() == 1);
        // Two per port, one per loop, two per channel.
        check!(specs.len() == 2 * 2 + 1 + 2);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn a_port_always_prepares_before_it_processes() {
        let desc = GraphDesc {
            ports: vec![port("solo", &[])],
            ..Default::default()
        };
        let (specs, map) = desc.build();
        let_assert!(Ok(schedule) = processing_order(&specs));
        let pos = |n: NodeIdx| schedule.iter().position(|s| s.contains(&n)).unwrap();
        check!(pos(map.port_prepare[0]) < pos(map.port_process[0]));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn a_channel_without_ports_still_orders_around_its_loop() {
        let desc = GraphDesc {
            ports: vec![],
            processors: vec![],
            loops: vec![LoopDesc::default()],
            channels: vec![ChannelDesc {
                loop_idx: LoopIdx(0),
                input_port: None,
                output_port: None,
            }],
        };
        let (specs, map) = desc.build();
        let_assert!(Ok(schedule) = processing_order(&specs));
        let pos = |n: NodeIdx| schedule.iter().position(|s| s.contains(&n)).unwrap();
        check!(pos(map.channel_prepare[0]) < pos(map.loop_process[0]));
        check!(pos(map.loop_process[0]) < pos(map.channel_process[0]));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn a_chain_of_internal_connections_is_ordered() {
        // a -> b -> c passthrough chain.
        let desc = GraphDesc {
            ports: vec![port("a", &[1]), port("b", &[2]), port("c", &[])],
            ..Default::default()
        };
        let (specs, map) = desc.build();
        let_assert!(Ok(schedule) = processing_order(&specs));
        let pos = |n: NodeIdx| schedule.iter().position(|s| s.contains(&n)).unwrap();
        check!(pos(map.port_process[0]) < pos(map.port_process[1]));
        check!(pos(map.port_process[1]) < pos(map.port_process[2]));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn a_passthrough_cycle_is_rejected() {
        // a -> b -> a cannot be ordered.
        let desc = GraphDesc {
            ports: vec![port("a", &[1]), port("b", &[0])],
            ..Default::default()
        };
        let (specs, _) = desc.build();
        check!(processing_order(&specs).is_err());
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn a_recording_channel_follows_its_input_ports_processing() {
        let desc = GraphDesc {
            ports: vec![port("in", &[])],
            processors: vec![],
            loops: vec![LoopDesc::default()],
            channels: vec![ChannelDesc {
                loop_idx: LoopIdx(0),
                input_port: Some(PortIdx(0)),
                output_port: None,
            }],
        };
        let (specs, map) = desc.build();
        let_assert!(Ok(schedule) = processing_order(&specs));
        let pos = |n: NodeIdx| schedule.iter().position(|s| s.contains(&n)).unwrap();
        // The channel can only capture the input once the port has applied gain.
        check!(pos(map.port_process[0]) < pos(map.channel_process[0]));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn a_playing_channel_precedes_its_output_ports_processing() {
        let desc = GraphDesc {
            ports: vec![port("out", &[])],
            processors: vec![],
            loops: vec![LoopDesc::default()],
            channels: vec![ChannelDesc {
                loop_idx: LoopIdx(0),
                input_port: None,
                output_port: Some(PortIdx(0)),
            }],
        };
        let (specs, map) = desc.build();
        let_assert!(Ok(schedule) = processing_order(&specs));
        let pos = |n: NodeIdx| schedule.iter().position(|s| s.contains(&n)).unwrap();
        check!(pos(map.channel_process[0]) < pos(map.port_process[0]));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn co_processed_loops_share_one_step() {
        let desc = GraphDesc {
            ports: vec![],
            processors: vec![],
            loops: vec![
                LoopDesc {
                    co_process_with: vec![LoopIdx(1)],
                },
                LoopDesc::default(),
                LoopDesc::default(),
            ],
            channels: vec![],
        };
        let (specs, map) = desc.build();
        let_assert!(Ok(schedule) = processing_order(&specs));
        // Loops 0 and 1 together, loop 2 alone.
        check!(schedule.len() == 2);
        let step_of = |n: NodeIdx| schedule.iter().find(|s| s.contains(&n)).unwrap();
        check!(step_of(map.loop_process[0]).len() == 2);
        check!(step_of(map.loop_process[2]).len() == 1);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn several_channels_on_one_loop_all_gate_it() {
        let desc = GraphDesc {
            ports: vec![],
            processors: vec![],
            loops: vec![LoopDesc::default()],
            channels: vec![
                ChannelDesc {
                    loop_idx: LoopIdx(0),
                    input_port: None,
                    output_port: None,
                },
                ChannelDesc {
                    loop_idx: LoopIdx(0),
                    input_port: None,
                    output_port: None,
                },
            ],
        };
        let (specs, map) = desc.build();
        let_assert!(Ok(schedule) = processing_order(&specs));
        let pos = |n: NodeIdx| schedule.iter().position(|s| s.contains(&n)).unwrap();
        // Both channels prepare before the loop runs, and settle after it.
        for i in 0..2 {
            check!(pos(map.channel_prepare[i]) < pos(map.loop_process[0]));
            check!(pos(map.loop_process[0]) < pos(map.channel_process[i]));
        }
    }
}
