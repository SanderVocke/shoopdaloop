#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;
#[shoop_test]
fn asynchronous_audio_bank_ownership_deadlines_reset_and_publication_cutoff() {
    for &c in rt_fpga_testbench::suites::audio_io::CONFIGS {
        rt_fpga_testbench::suites::audio_io::run(c).unwrap();
    }
}

#[shoop_test]
fn i2s_master_exchanges_stereo_pcm_with_one_bit_delay() {
    use rt_fpga_testbench::{model::Design, sim::Sim};

    let root = rt_fpga_testbench::root();
    let compiled = Design::new(
        "i2s_duplex",
        vec![root.join("fpga/audio_io/rtl/i2s_duplex.sv")],
    )
    .build(&root.join("work/marlin-suite/audio-io"))
    .unwrap();
    let mut sim = Sim::new(compiled.session(None).unwrap(), &[("audio_clk", 81_380, 0)]);
    sim.model.set("reset", 1);
    sim.idle(2);
    sim.model.set("reset", 0);

    let rx = [0x12_3456_u32, 0xed_cbaa];
    let rx_fixed = (u64::from(0xffdb_9754_u32) << 32) | u64::from(0x0024_68ac_u32);
    let tx = [
        ([0xfc00_0000_u32, 0x0400_0000], [0x80_0000_u32, 0x7f_ffff]),
        ([0x0012_3456, 0xffab_cdf0], [0x09_1a2b, 0xd5_e6f8]),
    ];
    for cycle in 0..3 * 256 {
        let frame = cycle / 256;
        let channel = cycle / 128 % 2;
        let bit = cycle / 4 % 32;
        let phase = cycle % 4;
        let payload = (1..=24).contains(&bit);
        let input = if payload {
            rx[channel] >> (24 - bit) & 1
        } else {
            1
        };
        sim.model.set("adc_data", u64::from(input));
        if let Some((samples, _)) = tx.get(frame) {
            sim.model.set(
                "tx_samples",
                u64::from(samples[0]) | (u64::from(samples[1]) << 32),
            );
        }
        sim.model.set("tx_valid", u64::from(cycle % 256 == 250));
        let (_, pins) = sim.tick(&[
            "bit_clk",
            "word_clk",
            "dac_data",
            "frame_prepare",
            "frame_tick",
            "rx_samples",
        ]);
        assert_eq!(pins.get("bit_clk"), u64::from(phase == 1 || phase == 2));
        assert_eq!(pins.get("frame_prepare"), u64::from(cycle % 256 == 237));
        assert_eq!(pins.get("frame_tick"), u64::from(cycle % 256 == 249));
        if phase == 1 {
            assert_eq!(pins.get("word_clk"), channel as u64);
            let expected = if frame > 0 && payload {
                tx[frame - 1].1[channel] >> (24 - bit) & 1
            } else {
                0
            };
            assert_eq!(pins.get("dac_data"), u64::from(expected), "cycle {cycle}");
        }
        if pins.get("frame_tick") != 0 {
            assert_eq!(pins.get("rx_samples"), rx_fixed);
        }
    }
}

#[shoop_test]
fn generated_audio_uses_fractional_frame_cadence_and_preserves_samples() {
    use rt_fpga_testbench::{model::Design, sim::Sim};

    const Q: usize = 4;
    const CLOCK: u64 = 1_004_000;
    let root = rt_fpga_testbench::root();
    let rtl = root.join("fpga/audio_io/rtl");
    let compiled = Design::new(
        "host_generated_audio",
        vec![
            root.join("fpga/host_bridge/rtl/host_generated_audio.v"),
            rtl.join("audio_io.sv"),
            rtl.join("audio_lane_ram.sv"),
        ],
    )
    .parameters(&[("QUANTUM", Q as u64), ("CORE_CLOCK_HZ", CLOCK)])
    .build(&root.join("work/marlin-suite/audio-io"))
    .unwrap();
    let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
    sim.model.set_all(&[("reset", 1), ("epoch", 1)]);
    sim.idle(4);
    sim.model.set_all(&[("reset", 0), ("enable", 1)]);

    let mut step = None;
    let mut samples = Vec::new();
    let mut ticks = Vec::new();
    for cycle in 1..=6000 {
        sim.model.set_all(&[
            ("sample_input_read", 0),
            ("sample_output_write", 0),
            ("quantum_done", 0),
            ("quantum_ready", 0),
        ]);
        if let Some(s) = step {
            if s == 3 * Q {
                sim.model.set("quantum_done", 1);
                step = None;
            } else {
                let index = s / 3;
                if s % 3 == 0 {
                    sim.model.set("sample_input_read", 1);
                    sim.model.set("sample_input_address", index as u64);
                } else {
                    let address = index + if s % 3 == 1 { 0 } else { Q };
                    let sample = sim.model.get("sample_input_data");
                    sim.model.set_all(&[
                        ("sample_output_write", 1),
                        ("sample_output_address", address as u64),
                        ("sample_output_data", sample),
                    ]);
                }
                step = Some(s + 1);
            }
        } else if sim.model.get("quantum_valid") != 0 {
            sim.model.set("quantum_ready", 1);
            step = Some(0);
        }
        sim.idle(1);
        assert_eq!(sim.model.get("audio_fault"), 0);
        if sim.model.get("tx_valid") != 0 {
            samples.push(sim.model.get("tx_samples"));
            ticks.push(cycle);
        }
    }
    assert_eq!(sim.model.get("missed_deadlines"), 0);
    assert!(sim.model.get("processed_quanta") > 60);
    assert_eq!(ticks.len() as u64, 6000 * 48_000 / CLOCK);
    assert!(ticks.windows(2).all(|w| matches!(w[1] - w[0], 20 | 21)));
    assert!(samples[..2 * Q].iter().all(|&v| v == 0));
    for (index, &sample) in samples[2 * Q..].iter().enumerate() {
        let expected = (index as u64) << 8;
        assert_eq!(sample, expected | (expected << 32));
    }
}

#[shoop_test]
fn board_i2s_adapter_echoes_left_input_and_reports_a_missed_deadline() {
    use rt_fpga_testbench::{model::Design, sim::Sim};

    const Q: usize = 4;
    let root = rt_fpga_testbench::root();
    let rtl = root.join("fpga/audio_io/rtl");
    let compiled = Design::new(
        "host_i2s_audio",
        vec![
            root.join("fpga/host_bridge/rtl/host_i2s_audio.v"),
            rtl.join("audio_io.sv"),
            rtl.join("audio_lane_ram.sv"),
            rtl.join("i2s_duplex.sv"),
        ],
    )
    .parameters(&[("QUANTUM", Q as u64)])
    .build(&root.join("work/marlin-suite/audio-io"))
    .unwrap();
    let mut sim = Sim::new(
        compiled.session(None).unwrap(),
        &[("clk", 10_000, 0), ("audio_ref", 81_380, 230)],
    );
    sim.model.set_all(&[("reset", 1), ("epoch", 1)]);
    sim.idle(40);
    sim.model.set_all(&[("reset", 0), ("enable", 1)]);

    let adc = [0x12_3456_u32, 0xec_ba98];
    let mut last_bclk = 0;
    let mut channel = 0;
    let mut slot = 0;
    let mut decoded = 0;
    let mut dac_samples = Vec::new();
    let mut step = None;
    for cycle in 0..110_000 {
        sim.model.set_all(&[
            ("sample_input_read", 0),
            ("sample_output_write", 0),
            ("quantum_done", 0),
            ("quantum_ready", 0),
        ]);
        if let Some(index) = step {
            if index == 3 * Q {
                sim.model.set("quantum_done", 1);
                step = None;
            } else {
                let frame = index / 3;
                if index % 3 == 0 {
                    sim.model.set("sample_input_read", 1);
                    sim.model.set("sample_input_address", frame as u64);
                } else {
                    let port = usize::from(index % 3 == 2);
                    sim.model.set_all(&[
                        ("sample_output_write", 1),
                        ("sample_output_address", (port * Q + frame) as u64),
                        ("sample_output_data", sim.model.get("sample_input_data")),
                    ]);
                }
                step = Some(index + 1);
            }
        } else if cycle < 65_000 && sim.model.get("quantum_valid") != 0 {
            sim.model.set("quantum_ready", 1);
            step = Some(0);
        }
        sim.idle(1);

        let bclk = sim.model.get("i2s_bclk");
        if last_bclk != bclk {
            if bclk == 0 {
                let next_channel = sim.model.get("i2s_lrclk") as usize;
                if next_channel != channel {
                    channel = next_channel;
                    slot = 0;
                    decoded = 0;
                } else {
                    slot += 1;
                }
                let bit = if (1..=24).contains(&slot) {
                    (adc[channel] >> (24 - slot)) & 1
                } else {
                    0
                };
                sim.model.set("i2s_adc_data", u64::from(bit));
            } else if (1..=24).contains(&slot) {
                decoded = (decoded << 1) | sim.model.get("i2s_dac_data") as u32;
                if slot == 24 {
                    dac_samples.push((cycle, decoded));
                }
            }
            last_bclk = bclk;
        }
        if cycle < 65_000 {
            assert_eq!(sim.model.get("audio_fault"), 0);
            assert_eq!(sim.model.get("missed_deadlines"), 0);
        }
    }
    let echoed = dac_samples
        .iter()
        .filter(|(cycle, _)| (35_000..65_000).contains(cycle));
    assert!(echoed.clone().count() > 20);
    assert!(echoed.into_iter().all(|(_, sample)| *sample == adc[0]));
    assert!(sim.model.get("missed_deadlines") > 0);
    assert_eq!(sim.model.get("audio_fault"), 1);
    assert!(
        dac_samples
            .iter()
            .rev()
            .take(4)
            .all(|(_, sample)| *sample == 0)
    );
}
