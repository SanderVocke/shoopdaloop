#[cfg(not(target_arch = "wasm32"))]
use anyhow::Context;
use anyhow::{anyhow, Result};
#[cfg(not(target_arch = "wasm32"))]
use serde::{Deserialize, Serialize};
use shoop_engine::oxisynth::SoundFontAsset;
use std::collections::BTreeMap;
use std::sync::Arc;

pub use shoop_app_api::SoundFontAssetDescriptor;

#[derive(Clone, Debug, Default)]
pub struct SoundFontLibrary {
    assets: BTreeMap<String, Arc<SoundFontAsset>>,
}

impl SoundFontLibrary {
    pub fn with_embedded() -> Result<Self> {
        let mut library = Self::default();
        let asset = Arc::new(SoundFontAsset::embedded()?);
        library.assets.insert(asset.sha256.clone(), asset);
        Ok(library)
    }

    pub fn import(
        &mut self,
        bytes: Arc<[u8]>,
        original_filename: impl Into<String>,
    ) -> Result<SoundFontAssetDescriptor> {
        let asset = Arc::new(SoundFontAsset::parse(bytes, original_filename)?);
        let descriptor = descriptor(&asset);
        self.assets.entry(asset.sha256.clone()).or_insert(asset);
        Ok(descriptor)
    }

    pub fn descriptors(&self) -> Arc<[SoundFontAssetDescriptor]> {
        self.assets
            .values()
            .map(|asset| descriptor(asset))
            .collect::<Vec<_>>()
            .into()
    }

    pub fn asset(&self, sha256: &str) -> Option<Arc<SoundFontAsset>> {
        self.assets.get(sha256).cloned()
    }

    pub fn user_assets(&self) -> Vec<Arc<SoundFontAsset>> {
        self.assets
            .values()
            .filter(|asset| asset.sha256 != shoop_engine::oxisynth::SOUNDFONT_SHA256)
            .cloned()
            .collect()
    }

    pub fn remove(&mut self, sha256: &str, referenced: bool) -> Result<bool> {
        if sha256 == shoop_engine::oxisynth::SOUNDFONT_SHA256 {
            return Err(anyhow!("the built-in SoundFont cannot be removed"));
        }
        if referenced {
            return Err(anyhow!("the SoundFont is still referenced"));
        }
        Ok(self.assets.remove(sha256).is_some())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_directory(path: &std::path::Path) -> Result<Self> {
        std::fs::create_dir_all(path).context("create SoundFont library")?;
        let mut library = Self::with_embedded()?;
        for entry in std::fs::read_dir(path).context("read SoundFont library")? {
            let entry = entry?;
            let file_name = entry.file_name().to_string_lossy().into_owned();
            if !file_name.ends_with(".sf2") || file_name.starts_with('.') {
                continue;
            }
            let bytes: Arc<[u8]> = std::fs::read(entry.path())?.into();
            let metadata_path = entry.path().with_extension("json");
            if !metadata_path.exists() {
                continue;
            }
            let original_filename =
                serde_json::from_slice::<PersistedAssetMetadata>(&std::fs::read(metadata_path)?)?
                    .original_filename;
            let descriptor = library.import(bytes, original_filename)?;
            if file_name != format!("{}.sf2", descriptor.sha256) {
                return Err(anyhow!(
                    "SoundFont payload filename does not match its digest"
                ));
            }
        }
        Ok(library)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn import_persistent(
        &mut self,
        directory: &std::path::Path,
        bytes: Arc<[u8]>,
        original_filename: impl Into<String>,
    ) -> Result<SoundFontAssetDescriptor> {
        std::fs::create_dir_all(directory).context("create SoundFont library")?;
        let asset = Arc::new(SoundFontAsset::parse(bytes.clone(), original_filename)?);
        if let Some(existing) = self.assets.get(&asset.sha256) {
            return Ok(descriptor(existing));
        }
        let descriptor = descriptor(&asset);
        let destination = directory.join(format!("{}.sf2", descriptor.sha256));
        if !destination.exists() {
            let temporary = directory.join(format!(".{}.tmp", descriptor.sha256));
            {
                use std::io::Write;
                let mut file = std::fs::File::create(&temporary)?;
                file.write_all(&bytes)?;
                file.sync_all()?;
            }
            std::fs::rename(&temporary, &destination)?;
        }
        let metadata_destination = directory.join(format!("{}.json", descriptor.sha256));
        let metadata_temporary = directory.join(format!(".{}.json.tmp", descriptor.sha256));
        std::fs::write(
            &metadata_temporary,
            serde_json::to_vec(&PersistedAssetMetadata {
                original_filename: descriptor.original_filename.to_string(),
            })?,
        )?;
        std::fs::rename(metadata_temporary, metadata_destination)?;
        self.assets.insert(asset.sha256.clone(), asset);
        Ok(descriptor)
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Deserialize, Serialize)]
struct PersistedAssetMetadata {
    original_filename: String,
}

fn descriptor(asset: &SoundFontAsset) -> SoundFontAssetDescriptor {
    SoundFontAssetDescriptor {
        sha256: asset.sha256.clone().into(),
        name: asset.name.clone().into(),
        original_filename: asset.original_filename.clone().into(),
        byte_len: asset.bytes.len(),
        presets: asset
            .presets
            .iter()
            .map(|preset| shoop_app_api::OxiSynthPresetDescriptor {
                bank: preset.bank,
                program: preset.program,
                name: preset.name.clone().into(),
            })
            .collect::<Vec<_>>()
            .into(),
        built_in: asset.sha256 == shoop_engine::oxisynth::SOUNDFONT_SHA256,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[shoop_wasm_test_support::shoop_test]
    fn identical_imports_are_content_deduplicated() {
        let bytes: Arc<[u8]> = Arc::from(shoop_engine::oxisynth::SOUNDFONT_BYTES);
        let mut library = SoundFontLibrary::default();
        let first = library.import(bytes.clone(), "first.sf2").unwrap();
        let second = library.import(bytes, "second.sf2").unwrap();
        assert_eq!(first.sha256, second.sha256);
        assert_eq!(library.descriptors().len(), 1);
        assert!(!first.presets.is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    #[cfg(not(target_arch = "wasm32"))]
    fn persistent_import_survives_restart_and_revalidates_digest() {
        let directory = tempfile::tempdir().unwrap();
        let bytes: Arc<[u8]> = Arc::from(shoop_engine::oxisynth::SOUNDFONT_BYTES);
        let mut library = SoundFontLibrary::default();
        let descriptor = library
            .import_persistent(directory.path(), bytes, "font.sf2")
            .unwrap();
        let restored = SoundFontLibrary::load_directory(directory.path()).unwrap();
        assert!(restored.asset(&descriptor.sha256).is_some());
        let metadata: PersistedAssetMetadata = serde_json::from_slice(
            &std::fs::read(directory.path().join(format!("{}.json", descriptor.sha256))).unwrap(),
        )
        .unwrap();
        assert_eq!(metadata.original_filename, "font.sf2");
    }

    #[shoop_wasm_test_support::shoop_test]
    #[cfg(not(target_arch = "wasm32"))]
    fn failed_persistent_import_does_not_publish_asset() {
        let directory = tempfile::tempdir().unwrap();
        let invalid_directory = directory.path().join("not-a-directory");
        std::fs::write(&invalid_directory, b"file").unwrap();
        let bytes: Arc<[u8]> = Arc::from(shoop_engine::oxisynth::SOUNDFONT_BYTES);
        let mut library = SoundFontLibrary::default();
        assert!(library
            .import_persistent(&invalid_directory, bytes, "font.sf2")
            .is_err());
        assert!(library.descriptors().is_empty());
    }
}
