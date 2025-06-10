use std::path::PathBuf;
use toml;

const SRC_DIR : &str = env!("CARGO_MANIFEST_DIR");

fn main() {
    let src = PathBuf::from(SRC_DIR);
    let root = src.ancestors().nth(3).unwrap();
    let metadata_file = root.join("metadata.toml");
    let metadata_content = std::fs::read_to_string(metadata_file).unwrap();
    let metadata: toml::Value = toml::from_str(&metadata_content).unwrap();

    let version = metadata["version"].as_str().unwrap();
    let description = metadata["description"].as_str().unwrap();

    println!("cargo:rustc-env=SHOOP_VERSION={}", version);
    println!("cargo:rustc-env=SHOOP_DESCRIPTION={}", description);
}
