//! Independent integer checks for interleaved smoothing histories and job boundaries.
use crate::{
    Result,
    bits::Bits,
    drivers::{config::Config, stream::Source},
    model::Design,
    random::Random,
    sim::Sim,
};
use serde_json::json;
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Copy, Default)]
struct History {
    last: i32,
    anchor: i32,
    elapsed: u32,
    duration: u32,
}
impl History {
    fn render(&mut self, data: &[i32], requested: u32, playing: bool, restart: bool) -> Vec<i32> {
        if !playing {
            *self = Self {
                elapsed: requested,
                duration: requested,
                ..Self::default()
            };
        } else if restart {
            self.anchor = self.last;
            self.elapsed = 0;
            self.duration = requested;
        }
        data.iter()
            .map(|&raw| {
                let result = if !playing {
                    0
                } else if self.elapsed >= self.duration {
                    raw
                } else {
                    let weight = (u64::from(self.elapsed) << 24) / u64::from(self.duration);
                    let delta = i64::from(raw) - i64::from(self.anchor);
                    (i64::from(self.anchor) + delta * weight as i64 / (1 << 24)) as i32
                };
                if playing {
                    self.last = result;
                    self.elapsed = self.elapsed.saturating_add(1).min(self.duration);
                }
                result
            })
            .collect()
    }
}
fn quantum(
    sim: &mut Sim<'_>,
    source: &Rc<RefCell<Source>>,
    data: &[i32],
    index: u64,
    duration: u32,
    playing: bool,
    restart: bool,
) -> (Vec<i32>, u64) {
    let config = Config::new("cfg", "cfg_space");
    for (address, &value) in data.iter().enumerate() {
        config.access(
            sim,
            0,
            address as u64,
            Some(&Bits::from_u64(u64::from(value as u32))),
            false,
        );
    }
    assert!(source.borrow().queue.is_empty());
    let mut input = Bits::zero(256);
    input.set(24, 16, 1);
    let mut smoothing = Bits::zero(256);
    smoothing
        .set(0, 8, 10)
        .set(8, 16, 1)
        .set(24, 16, 2)
        .set(64, 32, index)
        .set(96, 32, u64::from(duration))
        .set(128, 1, u64::from(playing))
        .set(129, 1, u64::from(restart));
    let mut output = Bits::zero(256);
    output.set(0, 8, 6).set(8, 16, 2);
    source.borrow_mut().queue.extend([input, smoothing, output]);
    let start = sim.cycles;
    sim.model
        .set_all(&[("run_quantum", data.len() as u64), ("run_valid", 1)]);
    let (before, _) = sim.tick(&["run_ready"]);
    assert_eq!(before.get("run_ready"), 1);
    sim.model.set("run_valid", 0);
    source.borrow_mut().enabled = true;
    sim.until(10_000, "smoothing completion", |m| m.get("done") != 0);
    let cycles = sim.cycles - start;
    assert!(
        source.borrow().queue.is_empty(),
        "unconsumed smoothing jobs"
    );
    assert_eq!(sim.model.get("error_code"), 0);
    source.borrow_mut().enabled = false;
    sim.model.set("job_valid", 0);
    let actual = (0..data.len())
        .map(|address| {
            config
                .access(sim, 1, address as u64, None, false)
                .get(0, 32) as i32
        })
        .collect();
    (actual, cycles)
}
pub fn histories_and_extremes() -> Result<()> {
    let root = crate::root();
    let sources = std::iter::once(root.join("fpga/state_processor/rtl/sp_ram.sv"))
        .chain(
            ["jp_sample_ram", "jp_divider", "job_processor"]
                .map(|n| root.join(format!("fpga/job_processor/rtl/{n}.sv"))),
        )
        .collect();
    let design = Design::new("job_processor", sources)
        .parameters(&[
            ("MAX_FRAMES", 32),
            ("BUFFERS", 8),
            ("INPUT_PORTS", 1),
            ("OUTPUT_PORTS", 1),
        ])
        .build(&root.join("work/marlin-suite/smoothing"))?;
    let mut sim = Sim::new(design.session(None)?, &[("clk", 10_000, 0)]);
    let source = Source::attach(&mut sim, "job_valid", "job_ready", "job_data", vec![]);
    let mut random = Random::new(983);
    let mut measurements = Vec::new();
    for epoch in 0..2 {
        source.borrow_mut().enabled = false;
        sim.model.set_all(&[
            ("reset", 1),
            ("run_valid", 0),
            ("job_valid", 0),
            ("job_enable", 1),
            ("run_chunk_frames", 1),
            ("run_buffers", 8),
            ("run_inputs", 1),
            ("run_outputs", 1),
            ("run_jobs", 3),
            ("cfg_valid", 0),
        ]);
        sim.idle(1);
        sim.model.set("reset", 0);
        sim.idle(1);
        let mut history = [History::default(); 8];
        for duration in [0, 1, 3, 31, 32, 33, 144, u32::MAX] {
            for ordinal in 0..12 {
                let index = [0, 7, 1][ordinal % 3];
                let playing = ordinal != 5;
                let restart = ordinal < 3 || matches!(ordinal, 6 | 10);
                let requested = if restart { duration } else { 17 };
                let data: Vec<_> = [i32::MIN, i32::MAX, -1, 0, 1]
                    .into_iter()
                    .chain((0..27).map(|_| random.signed()))
                    .collect();
                let expected = history[index].render(&data, requested, playing, restart);
                let (actual, cycles) = quantum(
                    &mut sim,
                    &source,
                    &data,
                    index as u64,
                    requested,
                    playing,
                    restart,
                );
                assert_eq!(
                    actual, expected,
                    "epoch {epoch}, duration {duration}, ordinal {ordinal}"
                );
                assert!(cycles < 1000, "smoothing took {cycles} cycles");
                measurements.push(cycles);
            }
        }
        let anchor = -987654;
        assert_eq!(
            quantum(&mut sim, &source, &[anchor], 6, 0, true, false).0,
            [anchor]
        );
        let mut elapsed = 0_u64;
        for (ordinal, length) in [1, 3, 17, 32].into_iter().enumerate() {
            let expected: Vec<_> = (0..length)
                .map(|_| {
                    let weight = (elapsed.min(33) << 24) / 33;
                    elapsed += 1;
                    (i64::from(anchor) + (12345 - i64::from(anchor)) * weight as i64 / (1 << 24))
                        as i32
                })
                .collect();
            let (actual, _) = quantum(
                &mut sim,
                &source,
                &vec![12345; length],
                6,
                if ordinal == 0 { 33 } else { 144 },
                true,
                ordinal == 0,
            );
            assert_eq!(
                actual, expected,
                "epoch {epoch}, irregular job length {length}"
            );
        }
    }
    let output = crate::artifact_directory("smoothing");
    std::fs::create_dir_all(&output)?;
    std::fs::write(
        output.join("smoothing-summary.json"),
        serde_json::to_vec_pretty(&json!({
            "quanta": measurements.len(), "max_cycles": measurements.iter().max(), "exact_samples": measurements.len() * 32
        }))?,
    )?;
    Ok(())
}
