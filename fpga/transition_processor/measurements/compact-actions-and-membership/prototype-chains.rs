use rt_fpga_testbench::{drivers::transition::Transition, sim::Sim, suites::transition};
fn main() -> rt_fpga_testbench::Result<()> {
    let flat = u64::from(std::env::var("SHOOP_TP_MEMBERSHIP").as_deref() == Ok("flat"));
    let depths = [64, 272, 400];
    let compiled = transition::design(depths)
        .parameters(&[("FLAT_MEMBERSHIP", flat)])
        .build(&rt_fpga_testbench::root().join("work/prototype-chains"))?;
    let mut sim = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
    let mut driver = Transition::new(depths);
    let mut rows = Vec::new();
    for case in transition::workloads() {
        let cycles = transition::compare(&mut driver, &mut sim, &case)?;
        rows.push(serde_json::json!({"name":case["name"],"cycles":cycles,"clock_hz":100_000_000}));
    }
    let path = rt_fpga_testbench::root().join(format!(
        "work/chains-{}.json",
        if flat != 0 { "flat" } else { "linked" }
    ));
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(path, serde_json::to_vec_pretty(&rows)?)?;
    Ok(())
}
