//! Guard readiness/generation and epoch changes at actual device activation, independent of ARM service.
use crate::{Result, bits::Bits, model::Design, sim::Sim};
use rt_fpga_adapter::{
    audio_image::Transfer,
    protocol::{get, put},
    resident_stages as wire,
};
use rt_runtime::{
    Epoch, Error,
    graph::{
        Catalog, PlanDescriptor, PreparedPlan,
        activation::{PreparedGraphs, StageId},
    },
};
use shoop_loops::{LoopId, runtime::NodePairStageId, topology::staging};

fn configure(s: &mut Sim<'_>, t: Transfer) -> u64 {
    s.model.set_all(&[
        ("configuration_valid", 1),
        ("configuration_opcode", get(&t.words, 632, 8)),
    ]);
    s.model.set_bits("configuration_data", &Bits::from(t.words));
    let (before, _) = s.tick(&["configuration_ready"]);
    assert_eq!(before.get("configuration_ready"), 1);
    s.model.set("configuration_valid", 0);
    s.until(100, "guard configuration reply", |m| {
        m.get("configuration_done") != 0
    });
    let error = s.model.get("configuration_error");
    s.idle(1);
    error
}
fn deliver(s: &mut Sim<'_>, t: Transfer, backend: Option<u64>) -> Bits {
    s.model.set_all(&[
        ("request_valid", 1),
        ("request_opcode", u64::from(t.opcode)),
    ]);
    s.model
        .set_bits("request_data", &Bits(t.words[..16].to_vec()));
    let (before, _) = s.tick(&["request_ready"]);
    assert_eq!(before.get("request_ready"), 1);
    s.model.set("request_valid", 0);
    s.until(100, "guard preflight", |m| {
        m.get("response_valid") != 0 || m.get("activation_valid") != 0
    });
    if let Some(error) = backend {
        assert_eq!(s.model.get("activation_valid"), 1);
        let epochs = (s.model.get("graph_epoch"), s.model.get("topology_epoch"));
        let nonce = s.model.get("activation_nonce");
        assert_ne!(nonce, 0);
        for _ in 0..8 {
            s.idle(1);
            assert_eq!(s.model.get("activation_valid"), 1);
            assert_eq!(s.model.get("activation_nonce"), nonce);
            assert_eq!(
                (s.model.get("graph_epoch"), s.model.get("topology_epoch")),
                epochs
            );
        }
        s.model.set("activation_ready", 1);
        s.idle(1);
        s.model.set("activation_ready", 0);
        s.idle(8);
        assert_eq!(s.model.get("response_valid"), 0);
        s.model
            .set_all(&[("activation_done", 1), ("activation_error", error)]);
        s.idle(1);
        s.model.set("activation_done", 0);
    } else {
        assert_eq!(
            s.model.get("activation_valid"),
            0,
            "refused action started execution"
        );
    }
    s.until(100, "guard terminal", |m| m.get("response_valid") != 0);
    let reply = s.model.bits("response_data");
    s.idle(1);
    assert_eq!(s.model.get("fault"), 0);
    reply
}
fn refused(s: &mut Sim<'_>, t: Transfer, code: u64) {
    let reply = deliver(s, t, None);
    assert_eq!(reply.get(504, 8), 1);
    assert_eq!(reply.get(0, 8), code);
}
pub fn guards() -> Result<()> {
    let root = crate::root();
    let compiled = Design::new(
        "audio_stages",
        vec![root.join("fpga/audio_io/rtl/audio_stages.sv")],
    )
    .build(&root.join("work/marlin-suite/stage-guards"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10000, 0)]);
    s.model.set("reset", 1);
    s.idle(2);
    s.model.set("reset", 0);
    s.idle(1);
    let config = rt_runtime::Config::default().prepare().unwrap();
    let mut graphs = PreparedGraphs::new(1, config).unwrap();
    let catalog = Catalog::new(config, &[]).unwrap();
    let plan = PreparedPlan::from_descriptor(config, PlanDescriptor::default()).unwrap();
    let graph = graphs
        .stage(0, Epoch(0), plan, &catalog)
        .map_err(|e| format!("{:?}", e.0))?;
    let pair = NodePairStageId::from_parts([31, 0, 1]).unwrap();
    let topo = staging::StageId::from_parts(41, 1).unwrap();
    let target = LoopId::from_parts(0, 7).unwrap();
    let mut spec = [0; 6];
    put(&mut spec, 0, 1, 1);
    put(&mut spec, 1, 32, 7);
    put(&mut spec, 33, 1, 1);
    put(&mut spec, 34, 32, 0);
    put(&mut spec, 66, 32, 7);
    put(&mut spec, 98, 1, 1);
    assert_eq!(configure(&mut s, wire::graph_ready(graph, Epoch(0))), 0);
    assert_eq!(configure(&mut s, wire::pair_ready(pair, Some(target))), 0);
    assert_eq!(configure(&mut s, wire::topology_ready(topo, spec, true)), 0);
    let initial = || wire::initial(graph, pair, topo, Epoch(0), Epoch(0), 2);
    let foreign = StageId::from_parts(graph.parts()[0] + 1, 0, 1).unwrap();
    assert_eq!(graphs.preflight(foreign, Epoch(1)), Err(Error::StaleEpoch));
    refused(
        &mut s,
        wire::initial(foreign, pair, topo, Epoch(1), Epoch(0), 2),
        10,
    );
    assert_eq!(graphs.preflight(foreign, Epoch(0)), Err(Error::InvalidId));
    refused(
        &mut s,
        wire::initial(foreign, pair, topo, Epoch(0), Epoch(0), 2),
        1,
    );
    assert_eq!(configure(&mut s, wire::pair_cancel(pair, false)), 0);
    refused(&mut s, initial(), 6);
    assert_eq!(configure(&mut s, wire::pair_cancel(pair, false)), 6);
    let pair = NodePairStageId::from_parts([31, 0, 2]).unwrap();
    assert_eq!(configure(&mut s, wire::pair_ready(pair, Some(target))), 0);
    refused(&mut s, initial(), 1);
    let initial = || wire::initial(graph, pair, topo, Epoch(0), Epoch(0), 2);
    refused(
        &mut s,
        wire::initial(graph, pair, topo, Epoch(0), Epoch(1), 2),
        10,
    );
    let refusal = deliver(&mut s, initial(), Some(1));
    assert_eq!((refusal.get(504, 8), refusal.get(0, 8)), (1, 1));
    assert_eq!(s.model.get("loaded"), 0);
    assert_eq!(s.model.get("graph_epoch"), 0);
    let result = deliver(&mut s, initial(), Some(0));
    assert_eq!(result.get(504, 8), 7);
    assert_eq!((result.get(256, 64), result.get(320, 64)), (1, 1));
    assert_eq!(graphs.activate(graph, Epoch(0)), Ok(Epoch(1)));
    assert_eq!(s.model.get("graph_epoch"), graphs.epoch().0);
    assert_eq!(s.model.get("loaded"), 1);
    assert_eq!(graphs.preflight(graph, Epoch(1)), Err(Error::InvalidState));
    refused(&mut s, wire::graph(graph, Epoch(1), Epoch(1), 3), 6);
    refused(&mut s, wire::graph(foreign, Epoch(0), Epoch(0), 3), 10);
    assert_eq!(configure(&mut s, wire::graph_release(graph)), 0);
    graphs.release(graph).unwrap();
    assert_eq!(configure(&mut s, wire::graph_release(graph)), 1);
    refused(&mut s, wire::graph(graph, Epoch(1), Epoch(1), 3), 1);
    let plan = PreparedPlan::from_descriptor(config, PlanDescriptor::default()).unwrap();
    let next = graphs
        .stage(0, Epoch(1), plan, &catalog)
        .map_err(|e| format!("{:?}", e.0))?;
    assert_eq!(configure(&mut s, wire::graph_ready(next, Epoch(1))), 0);
    refused(&mut s, wire::graph(graph, Epoch(1), Epoch(1), 3), 1);
    let result = deliver(&mut s, wire::graph(next, Epoch(1), Epoch(1), 3), Some(0));
    assert_eq!(graphs.activate(next, Epoch(1)), Ok(Epoch(2)));
    assert_eq!(result.get(504, 8), 6);
    assert_eq!((result.get(96, 64), result.get(160, 64)), (2, 1));
    assert_eq!(configure(&mut s, wire::pair_cancel(pair, false)), 6);
    assert_eq!(configure(&mut s, wire::pair_cancel(pair, true)), 0);
    assert_eq!(configure(&mut s, wire::topology_cancel(topo, false)), 6);
    assert_eq!(configure(&mut s, wire::topology_cancel(topo, true)), 0);
    println!(
        "stage guards: authoritative delayed commits, exact graph epoch/error parity, cancelled/released/recycled identities, separate initial and graph epochs, backend refusal preserves owners"
    );
    Ok(())
}
