mod support;

use rt_runtime::graph::*;
use rt_runtime::planner::*;
use rt_runtime::{Config, Error, NodeId, PlannerId, PreparedConfig};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn node(slot: u32) -> NodeId {
    NodeId::from_parts(slot, 1).unwrap()
}
fn planner() -> PlannerSpec {
    PlannerSpec {
        id: PlannerId::from_parts(0, 1).unwrap(),
        decision_slots: 2,
        observation_capacity: 2,
        operation_bytes: 32,
    }
}
fn nodes() -> Vec<NodeSpec> {
    (0..3)
        .map(|slot| NodeSpec {
            id: node(slot),
            ports: PortCounts {
                audio_inputs: 1,
                audio_outputs: 1,
                ..PortCounts::default()
            },
        })
        .collect()
}
fn consumers() -> Vec<ConsumerSpec> {
    [0, 2]
        .into_iter()
        .map(|slot| ConsumerSpec {
            node: node(slot),
            access: ConsumerAccess {
                decision_required: true,
                observations: 1,
                operation_bytes: 16,
            },
        })
        .collect()
}
fn graph() -> GraphDescription {
    GraphDescription {
        nodes: vec![node(2), node(0), node(1)],
        connections: vec![
            Connection {
                source: Endpoint::node(node(0), PortType::Audio, 0),
                destination: Endpoint::node(node(1), PortType::Audio, 0),
            },
            Connection {
                source: Endpoint::node(node(1), PortType::Audio, 0),
                destination: Endpoint::node(node(2), PortType::Audio, 0),
            },
        ],
    }
}
fn bindings() -> Vec<DecisionBinding> {
    vec![
        DecisionBinding {
            planner: planner().id,
            consumer: node(0),
            decision_slot: 0,
        },
        DecisionBinding {
            planner: planner().id,
            consumer: node(2),
            decision_slot: 0,
        },
    ]
}
fn catalog() -> Catalog {
    Catalog::with_planner(
        Config::default().prepare().unwrap(),
        &nodes(),
        planner(),
        &consumers(),
    )
    .unwrap()
}
fn validate(plan: &PreparedPlan, catalog: &Catalog, budget: usize) -> (ValidationStatus, usize) {
    let mut validator = PlanValidator::new(catalog.config());
    assert_no_heap(|| validator.begin());
    let mut count = 0;
    loop {
        let progress = assert_no_heap(|| validator.advance(plan, catalog, budget));
        assert!(progress.steps <= budget);
        count += progress.steps;
        if progress.status != ValidationStatus::Pending {
            return (progress.status, count);
        }
        assert!(progress.steps != 0);
    }
}
fn raw(
    config: PreparedConfig,
    signal: &PreparedPlan,
    planning: PlanningDescriptor,
) -> PreparedPlan {
    PreparedPlan::from_planned_descriptor(config, signal.descriptor().clone(), planning).unwrap()
}

#[shoop_test]
fn explicit_bindings_are_separate_from_signal_edges_and_canonical() {
    let catalog = catalog();
    let original = compile_planned(&catalog, &graph(), &bindings()).unwrap();
    assert_eq!(original.descriptor().connections.len(), 2);
    assert_eq!(original.planning().step.unwrap().before_step, 0);
    assert_eq!(original.planning().bindings.len(), 2);
    assert_eq!(validate(&original, &catalog, 1).0, ValidationStatus::Valid);
    let mut reversed = graph();
    reversed.nodes.reverse();
    reversed.connections.reverse();
    let mut reversed_bindings = bindings();
    reversed_bindings.reverse();
    let reordered = compile_planned(&catalog, &reversed, &reversed_bindings).unwrap();
    assert_eq!(original.descriptor(), reordered.descriptor());
    assert_eq!(original.planning(), reordered.planning());
    assert_eq!(
        catalog.consumer(node(1)).unwrap(),
        ConsumerAccess::default()
    );
}

#[shoop_test]
fn incremental_validation_work_is_partition_independent() {
    let catalog = catalog();
    let plan = compile_planned(&catalog, &graph(), &bindings()).unwrap();
    let expected = validate(&plan, &catalog, 1);
    for budget in [2, 7, 64, usize::MAX] {
        assert_eq!(validate(&plan, &catalog, budget), expected);
    }
}

#[shoop_test]
fn receiver_rejects_missing_stale_and_misordered_planning_prefix() {
    let catalog = catalog();
    let valid = compile_planned(&catalog, &graph(), &bindings()).unwrap();
    let mut cases = Vec::new();
    let mut missing = valid.planning().clone();
    missing.step = None;
    cases.push((missing, Error::InvalidGraph));
    let mut stale = valid.planning().clone();
    stale.step.as_mut().unwrap().planner.id = PlannerId::from_parts(0, 2).unwrap();
    cases.push((stale, Error::InvalidId));
    let mut wrong_signature = valid.planning().clone();
    wrong_signature
        .step
        .as_mut()
        .unwrap()
        .planner
        .decision_slots += 1;
    cases.push((wrong_signature, Error::InvalidGraph));
    let mut after_consumer = valid.planning().clone();
    after_consumer.step.as_mut().unwrap().before_step = 1;
    cases.push((after_consumer, Error::InvalidGraph));
    for (description, error) in cases {
        let plan = raw(catalog.config(), &valid, description);
        assert_eq!(
            validate(&plan, &catalog, 1).0,
            ValidationStatus::Invalid(error)
        );
    }
}

#[shoop_test]
fn receiver_rejects_duplicate_missing_stale_and_out_of_range_bindings() {
    let catalog = catalog();
    let valid = compile_planned(&catalog, &graph(), &bindings()).unwrap();
    let mut cases = Vec::new();
    let mut missing = valid.planning().clone();
    missing.bindings.pop();
    cases.push((missing, Error::InvalidGraph));
    let mut duplicate = valid.planning().clone();
    duplicate.bindings.insert(1, duplicate.bindings[0]);
    cases.push((duplicate, Error::InvalidGraph));
    let mut reversed = valid.planning().clone();
    reversed.bindings.reverse();
    cases.push((reversed, Error::InvalidGraph));
    let mut stale = valid.planning().clone();
    stale.bindings[0].consumer = NodeId::from_parts(0, 2).unwrap();
    cases.push((stale, Error::InvalidId));
    let mut stale_planner = valid.planning().clone();
    stale_planner.bindings[0].planner = PlannerId::from_parts(0, 2).unwrap();
    cases.push((stale_planner, Error::InvalidId));
    let mut invalid_slot = valid.planning().clone();
    invalid_slot.bindings[0].decision_slot = usize::MAX;
    cases.push((invalid_slot, Error::InvalidGraph));
    for (description, error) in cases {
        let plan = raw(catalog.config(), &valid, description);
        assert_eq!(
            validate(&plan, &catalog, 2).0,
            ValidationStatus::Invalid(error)
        );
    }
}

#[shoop_test]
fn observation_reservations_are_aggregate_storage_not_per_node_work_quotas() {
    let config = Config::default().prepare().unwrap();
    let catalog = Catalog::with_planner(
        config,
        &nodes(),
        PlannerSpec {
            observation_capacity: 1,
            ..planner()
        },
        &consumers(),
    )
    .unwrap();
    assert_eq!(
        compile_planned(&catalog, &graph(), &bindings()).err(),
        Some(Error::Capacity)
    );
    let single = GraphDescription {
        nodes: vec![node(0)],
        connections: vec![],
    };
    let plan = compile_planned(&catalog, &single, &bindings()[..1]).unwrap();
    assert_eq!(validate(&plan, &catalog, 1).0, ValidationStatus::Valid);
    assert_eq!(
        compile_planned(&catalog, &single, &bindings()).err(),
        Some(Error::InvalidId)
    );
}

#[shoop_test]
fn signal_only_catalog_rejects_planning_metadata_without_changing_legacy_plans() {
    let config = Config::default().prepare().unwrap();
    let catalog = Catalog::new(config, &nodes()).unwrap();
    let legacy = compile(&catalog, &graph()).unwrap();
    assert_eq!(legacy.planning(), &PlanningDescriptor::default());
    assert_eq!(validate(&legacy, &catalog, 1).0, ValidationStatus::Valid);
    let metadata = PlanningDescriptor {
        step: Some(PlanningStep {
            planner: planner(),
            before_step: 0,
        }),
        bindings: bindings(),
    };
    assert_eq!(
        validate(&raw(config, &legacy, metadata), &catalog, 1).0,
        ValidationStatus::Invalid(Error::InvalidGraph)
    );
}

#[shoop_test]
fn prepared_staging_reuses_bindings_without_heap_activity_or_stale_metadata() {
    let catalog = catalog();
    let valid = compile_planned(&catalog, &graph(), &bindings()).unwrap();
    let mut staged = PreparedPlan::with_capacity(catalog.config()).unwrap();
    assert_no_heap(|| {
        for _ in 0..100 {
            staged.begin_descriptor().unwrap();
            staged.set_planning_step(valid.planning().step);
            for &step in &valid.descriptor().steps {
                staged.push_step(step).unwrap();
            }
            for &edge in &valid.descriptor().connections {
                staged.push_connection(edge).unwrap();
            }
            for &binding in &valid.planning().bindings {
                staged.push_binding(binding).unwrap();
            }
        }
    });
    assert_eq!(validate(&staged, &catalog, 1).0, ValidationStatus::Valid);
    assert_no_heap(|| staged.begin_descriptor()).unwrap();
    assert_eq!(staged.planning(), &PlanningDescriptor::default());
    assert!(!staged.contains(node(0)));
    assert_no_heap(|| {
        for _ in 0..catalog.config().limits().max_nodes {
            staged.push_binding(bindings_value()).unwrap();
        }
        assert_eq!(staged.push_binding(bindings_value()), Err(Error::Capacity));
    });
}
fn bindings_value() -> DecisionBinding {
    DecisionBinding {
        planner: planner().id,
        consumer: node(0),
        decision_slot: 0,
    }
}

#[shoop_test]
fn malformed_catalog_and_empty_clock_only_planner_have_explicit_results() {
    let config = Config::default().prepare().unwrap();
    assert_eq!(PlannerId::from_parts(0, 0), Err(Error::InvalidId));
    assert_eq!(
        Catalog::with_planner(
            config,
            &nodes(),
            PlannerSpec {
                decision_slots: usize::MAX,
                ..planner()
            },
            &[]
        )
        .err(),
        Some(Error::Capacity)
    );
    let mut duplicate = consumers();
    duplicate.push(duplicate[0]);
    assert_eq!(
        Catalog::with_planner(config, &nodes(), planner(), &duplicate).err(),
        Some(Error::InvalidGraph)
    );
    let clock = Catalog::with_planner(
        config,
        &[],
        PlannerSpec {
            decision_slots: 0,
            observation_capacity: 0,
            operation_bytes: 0,
            ..planner()
        },
        &[],
    )
    .unwrap();
    let plan = compile_planned(&clock, &GraphDescription::default(), &[]).unwrap();
    assert_eq!(validate(&plan, &clock, 1).0, ValidationStatus::Valid);
}
