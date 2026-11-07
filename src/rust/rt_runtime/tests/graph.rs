mod support;

use rt_runtime::graph::*;
use rt_runtime::{Config, Error, NodeId};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn id(slot: u32) -> NodeId {
    NodeId::from_parts(slot, 1).unwrap()
}
fn ports() -> PortCounts {
    PortCounts {
        audio_inputs: 2,
        audio_outputs: 2,
        midi_inputs: 1,
        midi_outputs: 1,
    }
}
fn catalog(count: u32) -> Catalog {
    let nodes: Vec<_> = (0..count)
        .map(|slot| NodeSpec {
            id: id(slot),
            ports: ports(),
        })
        .collect();
    Catalog::new(
        Config {
            max_nodes: 8,
            ..Config::default()
        }
        .prepare()
        .unwrap(),
        &nodes,
    )
    .unwrap()
}
fn edge(from: Option<u32>, to: Option<u32>, kind: PortType) -> Connection {
    Connection {
        source: Endpoint {
            node: from.map(id),
            kind,
            port: 0,
        },
        destination: Endpoint {
            node: to.map(id),
            kind,
            port: 0,
        },
    }
}
fn validate(catalog: &Catalog, plan: &PreparedPlan, budget: usize) -> (ValidationStatus, usize) {
    let mut validator = PlanValidator::new(catalog.config());
    assert_eq!(
        validator.advance(plan, catalog, 1).status,
        ValidationStatus::Invalid(Error::InvalidState)
    );
    assert_no_heap(|| validator.begin());
    assert_eq!(
        assert_no_heap(|| validator.advance(plan, catalog, 0)).steps,
        0
    );
    let mut total = 0;
    for _ in 0..10_000 {
        let progress = assert_no_heap(|| validator.advance(plan, catalog, budget));
        assert!(progress.steps <= budget);
        total += progress.steps;
        if progress.status != ValidationStatus::Pending {
            return (progress.status, total);
        }
    }
    panic!("validator did not terminate within its declared step bound");
}

#[shoop_test]
fn typed_dag_compilation_is_deterministic_and_validates_one_step_at_a_time() {
    let catalog = catalog(3);
    let mut graph = GraphDescription {
        nodes: vec![id(2), id(0), id(1)],
        connections: vec![
            edge(None, Some(0), PortType::Audio),
            edge(Some(0), Some(1), PortType::Audio),
            edge(Some(0), Some(2), PortType::Audio),
            edge(Some(1), None, PortType::Audio),
            edge(Some(2), None, PortType::Audio),
            edge(None, Some(0), PortType::Midi),
            edge(Some(0), Some(2), PortType::Midi),
            edge(Some(2), None, PortType::Midi),
        ],
    };
    let first = compile(&catalog, &graph).unwrap();
    assert_eq!(
        first
            .descriptor()
            .steps
            .iter()
            .map(|step| step.node)
            .collect::<Vec<_>>(),
        vec![id(0), id(1), id(2)]
    );
    graph.nodes.reverse();
    graph.connections.reverse();
    let second = compile(&catalog, &graph).unwrap();
    assert_eq!(first.descriptor(), second.descriptor());
    assert_eq!(validate(&catalog, &first, 1).0, ValidationStatus::Valid);
    let (status, steps) = validate(&catalog, &first, 3);
    assert_eq!(status, ValidationStatus::Valid);
    assert_eq!(steps, validate(&catalog, &first, 1).1);
    assert!(first.contains(id(2)));
    assert!(!first.contains(id(2).checked_next_generation().unwrap()));
    assert!(!first.contains(id(7)));
}

#[shoop_test]
fn compile_rejects_cycles_duplicates_missing_nodes_and_incompatible_ports() {
    let catalog = catalog(2);
    let mut graph = GraphDescription {
        nodes: vec![id(0), id(1)],
        connections: vec![
            edge(Some(0), Some(1), PortType::Audio),
            edge(Some(1), Some(0), PortType::Audio),
        ],
    };
    assert!(matches!(compile(&catalog, &graph), Err(Error::Cycle)));
    graph.connections = vec![edge(Some(0), Some(0), PortType::Audio)];
    assert!(matches!(compile(&catalog, &graph), Err(Error::Cycle)));
    graph.connections = vec![edge(Some(0), Some(1), PortType::Audio); 2];
    assert!(matches!(
        compile(&catalog, &graph),
        Err(Error::InvalidGraph)
    ));
    graph.connections = vec![edge(Some(0), Some(1), PortType::Audio)];
    graph.connections[0].destination.kind = PortType::Midi;
    assert!(matches!(compile(&catalog, &graph), Err(Error::PortType)));
    graph.connections = vec![edge(Some(0), None, PortType::Audio)];
    graph.nodes = vec![id(1)];
    assert!(matches!(compile(&catalog, &graph), Err(Error::InvalidId)));
    graph.nodes = vec![id(0), id(0)];
    assert!(matches!(
        compile(&catalog, &graph),
        Err(Error::InvalidGraph)
    ));
    graph.nodes = vec![id(0).checked_next_generation().unwrap()];
    assert!(matches!(compile(&catalog, &graph), Err(Error::InvalidId)));
    graph.nodes = vec![id(0)];
    graph.connections[0].source.port = 2;
    assert!(matches!(
        compile(&catalog, &graph),
        Err(Error::InvalidGraph)
    ));
}

#[shoop_test]
fn catalog_checks_total_port_capacity_and_generational_slots() {
    let config = Config {
        max_nodes: 2,
        max_ports: 6,
        ..Config::default()
    }
    .prepare()
    .unwrap();
    assert!(Catalog::new(config, &[]).is_ok());
    assert!(matches!(
        Catalog::new(
            config,
            &[NodeSpec {
                id: id(0),
                ports: ports()
            }]
        ),
        Err(Error::Capacity)
    ));
    let config = Config::default().prepare().unwrap();
    assert!(matches!(
        Catalog::new(
            config,
            &[NodeSpec {
                id: id(0),
                ports: ports()
            }; 2]
        ),
        Err(Error::InvalidId)
    ));
    let huge = PortCounts {
        audio_inputs: usize::MAX,
        ..ports()
    };
    assert_eq!(huge.total(), Err(Error::Capacity));
    assert!(matches!(
        Catalog::new(
            config,
            &[NodeSpec {
                id: id(0),
                ports: huge
            }]
        ),
        Err(Error::Capacity)
    ));
    assert!(matches!(
        Catalog::new(
            config,
            &[NodeSpec {
                id: id(1000),
                ports: ports()
            }]
        ),
        Err(Error::InvalidId)
    ));
}

#[shoop_test]
fn receiver_rejects_malformed_descriptors_under_heap_and_work_guards() {
    let catalog = catalog(2);
    let make = |steps, connections| {
        PreparedPlan::from_descriptor(catalog.config(), PlanDescriptor { steps, connections })
            .unwrap()
    };
    let plans = [
        make(
            vec![NodeStep {
                node: id(0),
                input_end: usize::MAX,
            }],
            vec![],
        ),
        make(
            vec![
                NodeStep {
                    node: id(0),
                    input_end: 0
                };
                2
            ],
            vec![],
        ),
        make(
            vec![NodeStep {
                node: id(0),
                input_end: 1,
            }],
            vec![edge(Some(1), Some(0), PortType::Audio)],
        ),
        make(
            vec![NodeStep {
                node: id(0),
                input_end: 1,
            }],
            vec![edge(Some(0), Some(0), PortType::Audio)],
        ),
        make(
            vec![NodeStep {
                node: id(0),
                input_end: 0,
            }],
            vec![edge(None, Some(0), PortType::Audio)],
        ),
        make(
            vec![NodeStep {
                node: id(0),
                input_end: 2,
            }],
            vec![edge(None, Some(0), PortType::Audio); 2],
        ),
        make(
            vec![
                NodeStep {
                    node: id(0),
                    input_end: 1,
                },
                NodeStep {
                    node: id(1),
                    input_end: 0,
                },
            ],
            vec![edge(None, Some(0), PortType::Audio)],
        ),
        make(
            vec![NodeStep {
                node: id(99),
                input_end: 0,
            }],
            vec![],
        ),
        make(
            vec![NodeStep {
                node: id(0).checked_next_generation().unwrap(),
                input_end: 0,
            }],
            vec![],
        ),
    ];
    for plan in plans {
        assert!(matches!(
            validate(&catalog, &plan, 1).0,
            ValidationStatus::Invalid(_)
        ));
    }
    let mut wrong_type = edge(None, None, PortType::Audio);
    wrong_type.destination.kind = PortType::Midi;
    assert_eq!(
        validate(&catalog, &make(vec![], vec![wrong_type]), 2).0,
        ValidationStatus::Invalid(Error::PortType)
    );
    let mut wrong_port = edge(None, None, PortType::Audio);
    wrong_port.source.port = 999;
    assert_eq!(
        validate(&catalog, &make(vec![], vec![wrong_port]), 2).0,
        ValidationStatus::Invalid(Error::InvalidGraph)
    );
    let other_config = Config::default().prepare().unwrap();
    let other = PreparedPlan::from_descriptor(other_config, PlanDescriptor::default()).unwrap();
    assert_eq!(
        validate(&catalog, &other, 2).0,
        ValidationStatus::Invalid(Error::InvalidConfig)
    );
}

#[shoop_test]
fn zero_node_host_passthrough_and_empty_graph_are_valid() {
    let catalog = catalog(0);
    for connections in [
        vec![],
        vec![
            edge(None, None, PortType::Audio),
            edge(None, None, PortType::Midi),
        ],
    ] {
        let plan = compile(
            &catalog,
            &GraphDescription {
                nodes: vec![],
                connections,
            },
        )
        .unwrap();
        assert_eq!(validate(&catalog, &plan, 1).0, ValidationStatus::Valid);
    }
}

#[shoop_test]
fn plan_envelope_rejects_oversize_storage_before_rendering() {
    let config = Config {
        max_nodes: 1,
        max_edges: 1,
        ..Config::default()
    }
    .prepare()
    .unwrap();
    assert!(matches!(
        PreparedPlan::from_descriptor(
            config,
            PlanDescriptor {
                steps: vec![
                    NodeStep {
                        node: id(0),
                        input_end: 0
                    };
                    2
                ],
                connections: vec![],
            }
        ),
        Err(Error::Capacity)
    ));
    assert!(matches!(
        PreparedPlan::from_descriptor(
            config,
            PlanDescriptor {
                steps: vec![],
                connections: vec![edge(None, None, PortType::Audio); 2],
            }
        ),
        Err(Error::Capacity)
    ));
}

#[shoop_test]
fn seeded_dags_validate_and_all_dependencies_precede_their_destinations() {
    let catalog = catalog(8);
    let mut seed = 12345u32;
    for _ in 0..256 {
        let mut graph = GraphDescription {
            nodes: (0..8).rev().map(id).collect(),
            connections: vec![],
        };
        for from in 0..8 {
            for to in from + 1..8 {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                if seed & 3 == 0 {
                    graph
                        .connections
                        .push(edge(Some(from), Some(to), PortType::Audio));
                }
            }
        }
        let plan = compile(&catalog, &graph).unwrap();
        let (status, work) = validate(&catalog, &plan, 3);
        assert_eq!(status, ValidationStatus::Valid);
        assert!(work <= 8 + 3 * 8 + graph.connections.len() + 8);
        for connection in graph.connections {
            let index = |node| {
                plan.descriptor()
                    .steps
                    .iter()
                    .position(|step| Some(step.node) == node)
                    .unwrap()
            };
            assert!(index(connection.source.node) < index(connection.destination.node));
        }
    }
}
