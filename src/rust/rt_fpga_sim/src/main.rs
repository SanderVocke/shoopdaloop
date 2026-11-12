use rt_audio_math::Q824;
use rt_backend_testbench::{
    production, rtl,
    spec::{Action, Samples, Scenario},
};
use rt_fpga_sim::{SimResult as Result, Simulator, runtime, value_words};
use std::{path::PathBuf, time::Instant};
fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let case = PathBuf::from(
        args.next()
            .unwrap_or_else(|| "src/rust/rt_fpga_sim/fixtures/instant-gains.json".into()),
    );
    let artifacts = PathBuf::from(args.next().unwrap_or_else(|| "work/marlin-runtime".into()));
    let wave = args.next().map(PathBuf::from);
    std::fs::create_dir_all(&artifacts)?;
    let scenario: Scenario = serde_json::from_slice(&std::fs::read(&case)?)?;
    scenario.validate()?;
    let expected = production::run::<Q824>(&scenario)?;
    let image = rtl::compile(&scenario)?;
    let start = Instant::now();
    let rt = runtime(&artifacts)?;
    let mut sim = Simulator::new(&rt, wave.as_deref(), true)?;
    let build = start.elapsed();
    let start = Instant::now();
    sim.load(&image)?;
    let load = start.elapsed();
    for (index, step) in scenario.steps.iter().enumerate() {
        for action in &step.actions {
            if let Action::Gain {
                node,
                target,
                duration,
                at,
                ..
            } = action
            {
                if *duration != 0 || *at != index as u64 * u64::from(scenario.quantum) {
                    return Err(
                        "direct demonstrator requires current-boundary instant gains".into(),
                    );
                }
                for &address in &image
                    .gains
                    .iter()
                    .find(|b| b.id == *node)
                    .ok_or("gain binding")?
                    .ramps
                {
                    sim.state(
                        address,
                        Some(value_words(rtl::Value::Ramp(rtl::Ramp {
                            start: *target,
                            target: *target,
                            duration: 0,
                            elapsed: 0,
                        }))),
                        false,
                    )?;
                }
            } else {
                return Err("standalone accepts gain actions only".into());
            }
        }
        let outputs = sim.run(&image, &step.inputs)?;
        if Samples::Q824(outputs) != expected.quanta[index].outputs {
            return Err(format!("audio mismatch quantum {index}; replay {case:?}").into());
        }
        if sim.frame() != expected.quanta[index].next_frame {
            return Err("frame mismatch".into());
        }
    }
    println!(
        "PASS {}: {} quanta, {} cycles; model build/load {:?}, configuration {:?}",
        scenario.name,
        scenario.steps.len(),
        sim.cycles,
        build,
        load
    );
    Ok(())
}
