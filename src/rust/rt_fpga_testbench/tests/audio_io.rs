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
