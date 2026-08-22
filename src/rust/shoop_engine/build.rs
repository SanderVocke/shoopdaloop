use std::collections::BTreeSet;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let soundfont_path = manifest_dir.join("../../../third_party/timgm6mb/TimGM6mb.sf2");
    println!("cargo:rerun-if-changed={}", soundfont_path.display());

    let mut file = fs::File::open(&soundfont_path).unwrap();
    let soundfont = soundfont::SoundFont2::load(&mut file)
        .unwrap()
        .sort_presets();
    let mut identities = BTreeSet::new();
    let mut generated = String::from("&[\n");
    for preset in soundfont.presets {
        let bank = preset.header.bank;
        let program = preset.header.preset;
        assert!(
            program <= 127,
            "SoundFont preset program {program} is invalid"
        );
        assert!(
            identities.insert((bank, program)),
            "duplicate SoundFont preset {bank}:{program}"
        );
        writeln!(
            generated,
            "    OxiSynthPresetDescriptor {{ id: OxiSynthPresetId {{ bank: {bank}, program: {program} }}, name: {:?} }},",
            preset.header.name
        )
        .unwrap();
    }
    generated.push_str("]\n");

    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("oxisynth_presets.rs");
    fs::write(output, generated).unwrap();
}
