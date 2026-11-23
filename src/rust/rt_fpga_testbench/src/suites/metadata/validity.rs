use super::*;
use serde_json::json;

pub fn workloads(width: u64) -> Result<serde_json::Value> {
    let mut cases = Vec::new();
    for (views, slots) in [(1_u64, 1_u64), (5, 512), (512, 512)] {
        let mut design = design("media_metadata_tb", width, 16, 0);
        for (key, value) in [
            ("VIEWS", views),
            ("SLOTS", slots),
            ("TABLE_PAGES", 128),
            ("TABLE_LINES", 8),
        ] {
            design.parameters.insert(key.into(), value.to_string());
        }
        let compiled = design.build(&crate::root().join("work/marlin-suite/metadata-validity"))?;
        let mut s = Sim::new(compiled.benchmark_session()?, &[("clk", 10_000, 0)]);
        let memory = Memory::attach(
            &mut s,
            Mode::Instant,
            8192,
            0x01000000,
            &crate::artifact_directory("metadata-validity"),
            19307,
        )?;
        let d = Metadata {
            port: Request::new(&mut s, ""),
            memory,
            slots,
        };
        let mut live = vec![0, (views - 1).min(1), views - 1];
        live.sort_unstable();
        live.dedup();
        let count = slots.min(17) as usize;
        let mut rows = Vec::new();
        let mut resets = Vec::new();
        let request = |s: &mut Sim<'_>,
                       rows: &mut Vec<serde_json::Value>,
                       kind: &str,
                       pass: u64,
                       round: u64,
                       command: [u64; 5]| {
            let [op, view, index, data, error] = command;
            let start = s.cycles;
            let reads = s.model.get("sim_reads");
            let writes = s.model.get("sim_writes");
            let value = d
                .port
                .transaction(
                    s,
                    &[
                        ("request_opcode", op),
                        ("request_view", view),
                        ("request_index", index),
                        ("request_data", data),
                    ],
                    error,
                    3,
                )
                .get(0, 32);
            rows.push(json!({"kind":kind,"pass":pass,"round":round,"opcode":op,
                "view":view,"index":index,"data":data,"error":error,"value":value,
                "cycles":s.cycles-start,"reads":s.model.get("sim_reads")-reads,
                "writes":s.model.get("sim_writes")-writes}));
            value
        };
        d.reset(&mut s);
        for pass in 0..2 {
            let start = s.cycles;
            s.until(10_000, "metadata reset admission", |m| {
                m.get("request_ready") != 0
            });
            resets.push(json!({"pass":pass,"ready_wait_cycles":s.cycles-start}));
            let mut expected = vec![vec![0_u64; count]; live.len()];
            for (v, &view) in live.iter().enumerate() {
                request(
                    &mut s,
                    &mut rows,
                    "reset_extent",
                    pass,
                    0,
                    [0, view, 0, 0, 1],
                );
                for (index, value) in expected[v].iter_mut().enumerate() {
                    *value = 0x80000000 + pass * 65536 + view * 32 + index as u64;
                    request(
                        &mut s,
                        &mut rows,
                        "create",
                        pass,
                        0,
                        [1, view, index as u64, *value, 0],
                    );
                }
            }
            for round in 0..if pass == 0 { 255 } else { 257 } {
                request(
                    &mut s,
                    &mut rows,
                    "invalidate",
                    pass,
                    round,
                    [9, 0, 0, 0, 0],
                );
                if round % 13 == 0 {
                    let v = round as usize % live.len().saturating_sub(1).max(1);
                    expected[v][0] += 1;
                    request(
                        &mut s,
                        &mut rows,
                        "rewrite",
                        pass,
                        round,
                        [1, live[v], 0, expected[v][0], 0],
                    );
                }
                if round % 31 == 0 {
                    request(
                        &mut s,
                        &mut rows,
                        "invalid_trim",
                        pass,
                        round,
                        [6, 0x80000000 | (views - 1).min(1), 0, 0, 1],
                    );
                }
                for (v, &view) in live.iter().enumerate() {
                    let sleeping = live.len() > 1 && v == live.len() - 1;
                    if sleeping && round < 255 {
                        continue;
                    }
                    let index = if round % 2 == 0 { 0 } else { count - 1 };
                    assert_eq!(
                        request(
                            &mut s,
                            &mut rows,
                            "read_live",
                            pass,
                            round,
                            [0, view, index as u64, 0, 0],
                        ),
                        expected[v][index]
                    );
                    if sleeping && round == 255 {
                        assert!(rows.last().unwrap()["reads"].as_u64().unwrap() > 0);
                    }
                }
            }
            if pass == 0 {
                d.port.set(
                    &mut s,
                    &[
                        ("request_valid", 1),
                        ("request_opcode", 9),
                        ("request_view", 0),
                        ("request_index", 0),
                        ("request_data", 0),
                    ],
                );
                let mut admitted = false;
                for _ in 0..d.port.limit {
                    if s.tick(&["request_ready"]).0.get("request_ready") != 0 {
                        admitted = true;
                        break;
                    }
                }
                assert!(admitted);
                d.port.set(
                    &mut s,
                    &[
                        ("request_valid", 0),
                        ("request_opcode", 15),
                        ("request_view", u64::from(u32::MAX)),
                        ("request_index", u64::from(u32::MAX)),
                        ("request_data", u64::from(u32::MAX)),
                    ],
                );
                s.idle(5);
                d.reset(&mut s);
            }
        }
        cases.push(json!({"views":views,"slots":slots,"live_views":live,
            "completed_invalidations":512,"aborted_invalidation":256,
            "reset_during_accepted_invalidation_after_clocks":5,"resets":resets,
            "measurements":rows}));
    }
    Ok(json!({"cases":cases,"axi_data_width":width,"seed":19307,
        "scope":"metadata/cache/AXI with direct Instant memory and randomized bus stalls; request assertion through held response acceptance; initialization measured separately",
        "validation":"independent live-view values survive512completed invalidations, high invalid trims and rewrites; a sleeping view must reload after256invalidations; accepted invalidation abort/reset clears every old extent despite stale RAM; reset admission is awaited",
        "provenance":crate::report::provenance()}))
}
