use crate::{production, rtl, spec::*};
use shoop_wasm_test_support::shoop_test;

fn scenario() -> Scenario {
    serde_json::from_str(include_str!(
        "../../../../fpga/verification/scenarios/mono-record.json"
    ))
    .unwrap()
}

#[shoop_test]
fn common_session_executes_both_production_policies_and_lowers_a_constant_program() {
    let scenario = scenario();
    let q = production::run::<rt_runtime::Q824>(&scenario).unwrap();
    let f = production::run::<f32>(&scenario).unwrap();
    let original = rtl::compile(&scenario).unwrap();
    assert_eq!(q.quanta.len(), 9);
    assert_eq!(q.media[0].frames, 64);
    for (a, b) in q.quanta.iter().zip(&f.quanta) {
        assert_eq!(a.states, b.states);
        assert_eq!(a.outcomes, b.outcomes);
        assert_eq!(a.events, b.events);
        assert_eq!(a.pending, b.pending);
        let (Samples::Q824(qa), Samples::Float(fa)) = (&a.outputs, &b.outputs) else {
            panic!("incorrect sample policy");
        };
        for (&q, &f) in qa.iter().flatten().zip(fa.iter().flatten()) {
            assert!((q as f64 / (1u64 << 24) as f64 - f as f64).abs() <= f64::from(f32::EPSILON));
        }
    }
    let mut changed = scenario.clone();
    changed.steps.clear();
    assert_eq!(
        rtl::compile(&changed).unwrap(),
        original,
        "control activity must not specialize the program"
    );
    assert_eq!(
        original.node_order,
        vec![
            Identity {
                slot: 0,
                generation: 1
            },
            Identity {
                slot: 1,
                generation: 1
            }
        ]
    );
}

#[shoop_test]
fn schema_documents_roundtrip_and_reject_unknown_duplicate_or_lossy_inputs() {
    let scenario = scenario();
    let encoded = serde_json::to_string(&scenario).unwrap();
    assert_eq!(
        serde_json::from_str::<Scenario>(&encoded).unwrap(),
        scenario
    );
    let image = rtl::compile(&scenario).unwrap();
    assert_eq!(
        serde_json::from_str::<rtl::Image>(&serde_json::to_string(&image).unwrap()).unwrap(),
        image
    );
    let result = production::run::<rt_runtime::Q824>(&scenario).unwrap();
    assert_eq!(
        serde_json::from_str::<Result>(&serde_json::to_string(&result).unwrap()).unwrap(),
        result
    );
    for source in [
        encoded.replacen("\"quantum\":32", "\"quantum\":32.0", 1),
        encoded.replacen("\"quantum\":32", "\"quantum\":32,\"quantum\":32", 1),
        encoded.replacen("{", "{\"extra\":0,", 1),
    ] {
        assert!(serde_json::from_str::<Scenario>(&source).is_err());
    }
    for (start, target, duration, elapsed) in [
        (i32::MIN, i32::MAX, 0, 0),
        (i32::MAX, i32::MIN, u32::MAX, u32::MAX),
    ] {
        let value = rtl::Value::Ramp(rtl::Ramp {
            start,
            target,
            duration,
            elapsed,
        });
        assert_eq!(
            serde_json::from_str::<rtl::Value>(&serde_json::to_string(&value).unwrap()).unwrap(),
            value
        );
    }
    let mut bad = scenario.clone();
    bad.format_version += 1;
    assert!(bad.validate().is_err());
    let mut bad = scenario.clone();
    bad.steps[0].inputs[0].pop();
    assert!(bad.validate().is_err());
    let mut bad = scenario.clone();
    bad.nodes.pop();
    assert!(bad.validate().is_err());
}

#[shoop_test]
fn audit_policy_is_bit_exact_and_counts_real_saturations_without_another_executor() {
    let scenario = scenario();
    let plain = production::run::<rt_runtime::Q824>(&scenario).unwrap();
    let audited = crate::audit::run(&scenario).unwrap();
    assert_eq!(audited.result, plain);
    assert_eq!(audited.quanta.len(), scenario.steps.len());
    assert!(audited
        .quanta
        .iter()
        .all(|q| q.counts.first_clip_operation.is_none()));
    let mut clip: Scenario = serde_json::from_str(include_str!(
        "../../../../fpga/verification/scenarios/gain-ramps.json"
    ))
    .unwrap();
    clip.nodes[0].operation = NodeKind::Gain {
        channels: 1,
        initial: 2 << 24,
    };
    for step in &mut clip.steps {
        step.actions.clear();
        step.inputs[0].fill(100 << 24);
    }
    let plain = production::run::<rt_runtime::Q824>(&clip).unwrap();
    let audited = crate::audit::run(&clip).unwrap();
    assert_eq!(audited.result, plain);
    for (i, q) in audited.quanta.iter().enumerate() {
        assert_eq!(q.frame, i as u64 * 32);
        assert_eq!(q.counts.clipped_products, 32);
        assert!(q.counts.first_clip_operation.is_some());
    }
}

#[shoop_test]
fn float_result_codec_refuses_nonfinite_and_overflowing_samples() {
    let invalid = Samples::Float(vec![vec![f32::INFINITY]]);
    assert!(serde_json::to_string(&invalid).is_err());
    assert!(
        serde_json::from_str::<Samples>(r#"{"encoding":"float","channels":[[1e39]]}"#).is_err()
    );
}
