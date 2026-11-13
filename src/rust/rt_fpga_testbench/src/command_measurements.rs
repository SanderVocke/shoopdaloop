//! Matched control costs against current or explicitly frozen transition RTL.
use crate::{
    Result,
    bits::Bits,
    drivers::transition::{self as abi, Transition},
    sim::Sim,
    suites::transition,
};
use serde_json::json;
use std::path::Path;

fn transaction(
    d: &mut Transition,
    s: &mut Sim<'_>,
    legacy: bool,
    opcode: u64,
    index: u64,
    data: &Bits,
) -> (u64, Bits) {
    if legacy {
        d.raw_command(s, opcode, index, data, Some(0), false)
    } else {
        d.command(s, opcode, index, data, Some(0), false)
    }
}
fn accepted(
    d: &mut Transition,
    s: &mut Sim<'_>,
    legacy: bool,
    opcode: u64,
    index: u64,
    data: &Bits,
) -> Bits {
    let (error, value) = transaction(d, s, legacy, opcode, index, data);
    assert_eq!(error, 0, "measurement opcode {opcode}");
    value
}
fn action(sequence: u64, stop: bool, at: u64) -> Bits {
    abi::command(&transition::command(
        0,
        sequence,
        if stop {
            json!({"kind":"stop"})
        } else {
            transition::play(0, true)
        },
        at,
        None,
        0,
    ))
}
pub fn run(requests: u64, source: Option<&Path>, legacy: bool, output: &Path) -> Result<()> {
    if !matches!(requests, 1 | 11 | 64 | 272) {
        return Err("measurement request depth must be 1, 11, 64 or 272".into());
    }
    if output.join("commands.json").exists() {
        return Err("measurement output already contains command results".into());
    }
    let original = source
        .map(Path::to_path_buf)
        .unwrap_or_else(|| crate::root().join("fpga/transition_processor/rtl"));
    if legacy
        && (source.is_none()
            || !std::fs::read_to_string(original.join("transition_processor.sv"))?
                .contains("S_SUBMIT_SCAN"))
    {
        return Err("legacy measurement requires explicitly frozen allocator RTL".into());
    }
    let frozen = output.join("source");
    std::fs::create_dir_all(&frozen)?;
    let depths = [64, requests, 400];
    let mut design = transition::design(depths);
    design.sources.clear();
    for name in ["tp_ram.sv", "tp_priority.sv", "transition_processor.sv"] {
        let target = frozen.join(name);
        std::fs::copy(original.join(name), &target)?;
        design.sources.push(target);
    }
    let compiled = design.build(&crate::root().join("work/marlin-suite/command-measurements"))?;
    let mut sim = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
    let mut driver = Transition::new(depths);
    driver.service_timings = (!legacy).then(Vec::new);
    let case = transition::case(
        "boundary-command-costs",
        vec![transition::spec(0, 64, None, false)],
        vec![],
        32,
        None,
        requests,
    );
    let case: rt_backend_testbench::transition_fixture::TransitionCase =
        serde_json::from_value(case)?;
    let image = rt_backend_testbench::transition_fixture::compile(&case.plan)
        .map_err(|error| format!("transition image: {error:?}"))?;
    driver.load(&mut sim, &serde_json::to_value(image)?);
    let mut repeated = Vec::new();
    for sequence in 1..=8 {
        let id = accepted(
            &mut driver,
            &mut sim,
            legacy,
            4,
            0,
            &action(sequence, false, 0),
        );
        let mut row = json!({"install":driver.last_cycles});
        for (opcode, name) in [(9, "process"), (12, "read_state"), (0, "status")] {
            accepted(&mut driver, &mut sim, legacy, opcode, 0, &Bits::default());
            row[name] = json!(driver.last_cycles);
        }
        let result = accepted(
            &mut driver,
            &mut sim,
            legacy,
            6,
            id.get(0, 32),
            &Bits::from_u64(id.get(32, 32)),
        );
        assert_eq!(result.get(37, 3), 1);
        row["collect"] = json!(driver.last_cycles);
        let id = accepted(
            &mut driver,
            &mut sim,
            legacy,
            4,
            0,
            &action(sequence + 10, false, 1 << 32),
        );
        let generation = Bits::from_u64(id.get(32, 32));
        accepted(&mut driver, &mut sim, legacy, 5, id.get(0, 32), &generation);
        row["cancel"] = json!(driver.last_cycles);
        let result = accepted(&mut driver, &mut sim, legacy, 6, id.get(0, 32), &generation);
        assert_eq!(result.get(37, 3), 3);
        row["cancelled_collect"] = json!(driver.last_cycles);
        repeated.push(row);
    }
    let mut identities = Vec::new();
    let mut dense_installs = Vec::new();
    for sequence in 100..100 + requests {
        identities.push(accepted(
            &mut driver,
            &mut sim,
            legacy,
            4,
            0,
            &action(sequence, true, 0),
        ));
        dense_installs.push(driver.last_cycles);
    }
    assert_eq!(
        transaction(
            &mut driver,
            &mut sim,
            legacy,
            4,
            0,
            &action(99 + requests, false, 0)
        )
        .0,
        4
    );
    let duplicate_cycles = driver.last_cycles;
    assert_eq!(
        transaction(
            &mut driver,
            &mut sim,
            legacy,
            4,
            0,
            &action(100 + requests, false, 0)
        )
        .0,
        2
    );
    let full_cycles = driver.last_cycles;
    accepted(&mut driver, &mut sim, legacy, 9, 0, &Bits::default());
    let dense_process = driver.last_cycles;
    assert_eq!(
        transaction(
            &mut driver,
            &mut sim,
            legacy,
            4,
            0,
            &action(101 + requests, false, 0)
        )
        .0,
        2
    );
    let retained_full_cycles = driver.last_cycles;
    let first = &identities[0];
    accepted(
        &mut driver,
        &mut sim,
        legacy,
        6,
        first.get(0, 32),
        &Bits::from_u64(first.get(32, 32)),
    );
    let reused = accepted(
        &mut driver,
        &mut sim,
        legacy,
        4,
        0,
        &action(101 + requests, false, 0),
    );
    assert_eq!(reused.get(0, 32), 0);
    let result = json!({"depths":depths,"repeated":repeated,"duplicate_cycles":duplicate_cycles,"full_cycles":full_cycles,"retained_full_cycles":retained_full_cycles,"dense_process":dense_process,"dense_installs":dense_installs,"reused_install_cycles":driver.last_cycles,"host_service_ns":driver.service_timings.take().unwrap_or_default(),"host_metadata_bytes":if legacy {None} else {driver.service.as_ref().map(|owner|owner.metadata_bytes())},"payload_bytes":{"install":80,"cancel":80,"collect_response":104}});
    std::fs::write(
        output.join("commands.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    std::fs::write(
        output.join("provenance.json"),
        serde_json::to_vec_pretty(&crate::report::provenance())?,
    )?;
    println!("Command measurements: {}", output.display());
    Ok(())
}
