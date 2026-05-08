use std::path::PathBuf;
use toml;

const SRC_DIR: &str = env!("CARGO_MANIFEST_DIR");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let src = PathBuf::from(SRC_DIR);
    let root = src
        .ancestors()
        .nth(3)
        .ok_or("Could not find project root")?;
    let metadata_file = root.join("metadata.toml");
    let metadata_content = std::fs::read_to_string(metadata_file)?;
    let metadata: toml::Value = toml::from_str(&metadata_content)?;

    let version = metadata["version"]
        .as_str()
        .ok_or("Version missing in metadata.toml")?;
    let description = metadata["description"]
        .as_str()
        .ok_or("Description missing in metadata.toml")?;

    println!("cargo:rustc-env=SHOOP_VERSION={}", version);
    println!("cargo:rustc-env=SHOOP_DESCRIPTION={}", description);
    Ok(())


    sdgsdg
}
