use shoop_settings::{
    decode_settings, encode_settings, SettingsDiagnostic, SettingsDocument, SettingsDraft,
    SettingsPersistenceState, SettingsRegistry, SettingsSnapshot, SettingsViewState,
};
use std::error::Error;
use std::fmt;
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc::{self, Receiver, TryRecvError};

#[derive(Debug, Eq, PartialEq)]
pub enum SettingsManagerError {
    Saving,
    RecoveryRequired,
    StaleRevision { expected: u64, actual: u64 },
    InvalidDraft(String),
    Storage(String),
}

impl fmt::Display for SettingsManagerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Saving => formatter.write_str("settings are already being saved"),
            Self::RecoveryRequired => formatter.write_str(
                "the stored settings were rejected; explicitly reset them before saving",
            ),
            Self::StaleRevision { expected, actual } => {
                write!(
                    formatter,
                    "stale settings revision {actual}; current is {expected}"
                )
            }
            Self::InvalidDraft(message) => write!(formatter, "invalid settings draft: {message}"),
            Self::Storage(message) => write!(formatter, "settings storage failed: {message}"),
        }
    }
}

impl Error for SettingsManagerError {}

#[cfg(not(target_arch = "wasm32"))]
struct PendingNativeSave {
    receiver: Receiver<Result<SettingsDocument, String>>,
}

pub struct SettingsManager {
    registry: Arc<SettingsRegistry>,
    writer_version: String,
    document: SettingsDocument,
    active: Arc<SettingsSnapshot>,
    diagnostics: Vec<SettingsDiagnostic>,
    storage_location: String,
    recovery_required: bool,
    persistence: SettingsPersistenceState,
    #[cfg(not(target_arch = "wasm32"))]
    path: PathBuf,
    #[cfg(not(target_arch = "wasm32"))]
    pending: Option<PendingNativeSave>,
}

impl SettingsManager {
    #[cfg(all(not(target_arch = "wasm32"), not(test)))]
    pub fn load(registry: SettingsRegistry, writer_version: impl Into<String>) -> Self {
        let writer_version = writer_version.into();
        match shoop_settings::default_settings_path() {
            Ok(path) => Self::load_from_path(registry, writer_version, path),
            Err(error) => Self::from_loaded(
                registry,
                writer_version,
                "unavailable native configuration path".to_owned(),
                Err(error.to_string()),
                PathBuf::new(),
            ),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_from_path(
        registry: SettingsRegistry,
        writer_version: impl Into<String>,
        path: PathBuf,
    ) -> Self {
        let loaded = shoop_settings::load_settings_file(&path).map_err(|error| error.to_string());
        Self::from_loaded(
            registry,
            writer_version.into(),
            path.display().to_string(),
            loaded,
            path,
        )
    }

    #[cfg(target_arch = "wasm32")]
    pub fn load(registry: SettingsRegistry, writer_version: impl Into<String>) -> Self {
        let loaded = browser_load();
        Self::from_loaded(
            registry,
            writer_version.into(),
            format!("localStorage key {}", shoop_settings::SETTINGS_STORAGE_KEY),
            loaded,
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn from_loaded(
        registry: SettingsRegistry,
        writer_version: String,
        storage_location: String,
        loaded: Result<Option<String>, String>,
        path: PathBuf,
    ) -> Self {
        let (document, active, diagnostics, recovery_required) =
            resolve_loaded(&registry, &writer_version, loaded);
        Self {
            registry: Arc::new(registry),
            writer_version,
            document,
            active,
            diagnostics,
            storage_location,
            recovery_required,
            persistence: SettingsPersistenceState::Idle,
            path,
            pending: None,
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn from_loaded(
        registry: SettingsRegistry,
        writer_version: String,
        storage_location: String,
        loaded: Result<Option<String>, String>,
    ) -> Self {
        let (document, active, diagnostics, recovery_required) =
            resolve_loaded(&registry, &writer_version, loaded);
        Self {
            registry: Arc::new(registry),
            writer_version,
            document,
            active,
            diagnostics,
            storage_location,
            recovery_required,
            persistence: SettingsPersistenceState::Idle,
        }
    }

    pub fn report_action_error(&mut self, message: impl Into<String>) {
        self.record_failure(message.into());
    }

    pub fn active(&self) -> Arc<SettingsSnapshot> {
        Arc::clone(&self.active)
    }

    pub fn view(&self) -> SettingsViewState {
        SettingsViewState {
            active: Arc::clone(&self.active),
            diagnostics: Arc::from(self.diagnostics.clone()),
            storage_location: self.storage_location.clone(),
            recovery_required: self.recovery_required,
            persistence: self.persistence,
        }
    }

    pub fn request_save(&mut self, draft: SettingsDraft) -> Result<(), SettingsManagerError> {
        if self.recovery_required {
            return Err(SettingsManagerError::RecoveryRequired);
        }
        self.begin_save(draft)
    }

    pub fn request_recovery(&mut self) -> Result<(), SettingsManagerError> {
        let snapshot = self.registry.defaults(self.active.revision());
        self.begin_save(SettingsDraft::from_snapshot(&snapshot))
    }

    fn begin_save(&mut self, draft: SettingsDraft) -> Result<(), SettingsManagerError> {
        if self.persistence == SettingsPersistenceState::Saving {
            return Err(SettingsManagerError::Saving);
        }
        if draft.base_revision() != self.active.revision() {
            return Err(SettingsManagerError::StaleRevision {
                expected: self.active.revision(),
                actual: draft.base_revision(),
            });
        }
        self.registry
            .validate_draft(&draft)
            .map_err(|error| SettingsManagerError::InvalidDraft(error.to_string()))?;
        let base = if self.recovery_required {
            SettingsDocument::empty(&self.writer_version)
        } else {
            self.document.clone()
        };
        let document = self
            .registry
            .document_from_draft(&base, &draft, &self.writer_version)
            .map_err(|error| SettingsManagerError::InvalidDraft(error.to_string()))?;
        let encoded = encode_settings(&document)
            .map_err(|error| SettingsManagerError::InvalidDraft(error.to_string()))?;
        self.persistence = SettingsPersistenceState::Saving;

        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.path.as_os_str().is_empty() {
                self.persistence = SettingsPersistenceState::Failed;
                return Err(SettingsManagerError::Storage(
                    "native configuration path is unavailable".to_owned(),
                ));
            }
            let path = self.path.clone();
            let (sender, receiver) = mpsc::channel();
            std::thread::Builder::new()
                .name("shoop-settings-save".to_owned())
                .spawn(move || {
                    let result = shoop_settings::save_settings_file(&path, &encoded)
                        .map(|()| document)
                        .map_err(|error| error.to_string());
                    let _ = sender.send(result);
                })
                .map_err(|error| {
                    self.persistence = SettingsPersistenceState::Failed;
                    SettingsManagerError::Storage(error.to_string())
                })?;
            self.pending = Some(PendingNativeSave { receiver });
            Ok(())
        }

        #[cfg(target_arch = "wasm32")]
        {
            match browser_save(&encoded) {
                Ok(()) => {
                    self.apply_saved(document);
                    Ok(())
                }
                Err(message) => {
                    self.record_failure(message.clone());
                    Err(SettingsManagerError::Storage(message))
                }
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn poll(&mut self) {
        let Some(pending) = &self.pending else {
            return;
        };
        let result = match pending.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("settings save worker disconnected".to_owned()),
        };
        self.pending = None;
        match result {
            Ok(document) => self.apply_saved(document),
            Err(message) => self.record_failure(message),
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn poll(&mut self) {}

    fn apply_saved(&mut self, document: SettingsDocument) {
        let next_revision = self.active.revision().wrapping_add(1);
        let resolved = self.registry.resolve(&document, next_revision);
        self.document = document;
        self.active = Arc::new(resolved.snapshot);
        self.diagnostics = resolved.diagnostics;
        self.recovery_required = false;
        self.persistence = SettingsPersistenceState::Saved;
    }

    fn record_failure(&mut self, message: String) {
        self.persistence = SettingsPersistenceState::Failed;
        self.diagnostics.push(SettingsDiagnostic {
            key: None,
            message: format!("Could not save settings: {message}"),
        });
    }
}

fn resolve_loaded(
    registry: &SettingsRegistry,
    writer_version: &str,
    loaded: Result<Option<String>, String>,
) -> (
    SettingsDocument,
    Arc<SettingsSnapshot>,
    Vec<SettingsDiagnostic>,
    bool,
) {
    match loaded {
        Ok(None) => {
            let document = SettingsDocument::empty(writer_version);
            let resolved = registry.resolve(&document, 1);
            (
                document,
                Arc::new(resolved.snapshot),
                resolved.diagnostics,
                false,
            )
        }
        Ok(Some(contents)) => match decode_settings(&contents) {
            Ok(document) => {
                let resolved = registry.resolve(&document, 1);
                (
                    document,
                    Arc::new(resolved.snapshot),
                    resolved.diagnostics,
                    false,
                )
            }
            Err(error) => {
                let document = SettingsDocument::empty(writer_version);
                let resolved = registry.resolve(&document, 1);
                (
                    document,
                    Arc::new(resolved.snapshot),
                    vec![SettingsDiagnostic {
                        key: None,
                        message: format!(
                            "Stored settings were rejected and were not overwritten: {error}"
                        ),
                    }],
                    true,
                )
            }
        },
        Err(message) => {
            let document = SettingsDocument::empty(writer_version);
            let resolved = registry.resolve(&document, 1);
            (
                document,
                Arc::new(resolved.snapshot),
                vec![SettingsDiagnostic {
                    key: None,
                    message: format!(
                        "Settings could not be read and were not overwritten: {message}"
                    ),
                }],
                true,
            )
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn browser_storage() -> Result<web_sys::Storage, String> {
    let window = web_sys::window().ok_or_else(|| "browser window is unavailable".to_owned())?;
    window
        .local_storage()
        .map_err(js_error)?
        .ok_or_else(|| "localStorage is unavailable".to_owned())
}

#[cfg(target_arch = "wasm32")]
fn browser_load() -> Result<Option<String>, String> {
    browser_storage()?
        .get_item(shoop_settings::SETTINGS_STORAGE_KEY)
        .map_err(js_error)
}

#[cfg(target_arch = "wasm32")]
fn browser_save(contents: &str) -> Result<(), String> {
    let inject_failure = web_sys::window()
        .and_then(|window| window.location().search().ok())
        .is_some_and(|search| search.contains("settings-save-failure=1"));
    if inject_failure {
        return Err("injected browser settings save failure".to_owned());
    }
    browser_storage()?
        .set_item(shoop_settings::SETTINGS_STORAGE_KEY, contents)
        .map_err(js_error)
}

#[cfg(target_arch = "wasm32")]
fn js_error(value: wasm_bindgen::JsValue) -> String {
    value
        .as_string()
        .unwrap_or_else(|| format!("browser storage exception: {value:?}"))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use shoop_settings::{SettingDefinition, SettingKey, SettingsRegistryBuilder};
    use std::time::{Duration, Instant};

    const VALUE: SettingKey<u32> = SettingKey::new("test.value");

    fn registry() -> SettingsRegistry {
        let mut builder = SettingsRegistryBuilder::default();
        builder
            .register(SettingDefinition::new(
                VALUE,
                2,
                "Test",
                "Value",
                "A test value",
            ))
            .unwrap();
        builder.finish()
    }

    fn wait(manager: &mut SettingsManager) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while manager.view().persistence == SettingsPersistenceState::Saving {
            manager.poll();
            assert!(Instant::now() < deadline, "settings save timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn first_run_save_and_restart_publish_only_after_commit() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut manager = SettingsManager::load_from_path(registry(), "test", path.clone());
        assert_eq!(manager.active().get(VALUE).unwrap(), 2);
        let mut draft = SettingsDraft::from_snapshot(&manager.active());
        draft.set(VALUE, 7);
        manager.request_save(draft).unwrap();
        assert_eq!(manager.active().get(VALUE).unwrap(), 2);
        assert_eq!(manager.view().persistence, SettingsPersistenceState::Saving);
        wait(&mut manager);
        assert_eq!(manager.active().get(VALUE).unwrap(), 7);
        assert_eq!(manager.active().revision(), 2);

        let restarted = SettingsManager::load_from_path(registry(), "test-2", path);
        assert_eq!(restarted.active().get(VALUE).unwrap(), 7);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn stale_draft_is_rejected_without_writing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut manager = SettingsManager::load_from_path(registry(), "test", path.clone());
        let stale = SettingsDraft::from_snapshot(&manager.active());
        let mut first = stale.clone();
        first.set(VALUE, 4);
        manager.request_save(first).unwrap();
        wait(&mut manager);
        assert_eq!(
            manager.request_save(stale).unwrap_err(),
            SettingsManagerError::StaleRevision {
                expected: 2,
                actual: 1,
            }
        );
        assert_eq!(manager.active().get(VALUE).unwrap(), 4);
        assert!(path.exists());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn rejected_source_requires_explicit_recovery_and_is_not_overwritten() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        std::fs::write(&path, "future-or-malformed").unwrap();
        let mut manager = SettingsManager::load_from_path(registry(), "test", path.clone());
        assert!(manager.view().recovery_required);
        assert_eq!(manager.active().get(VALUE).unwrap(), 2);
        assert_eq!(
            manager
                .request_save(SettingsDraft::from_snapshot(&manager.active()))
                .unwrap_err(),
            SettingsManagerError::RecoveryRequired
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "future-or-malformed"
        );

        manager.request_recovery().unwrap();
        wait(&mut manager);
        assert!(!manager.view().recovery_required);
        assert_eq!(manager.active().get(VALUE).unwrap(), 2);
        assert!(decode_settings(&std::fs::read_to_string(path).unwrap()).is_ok());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn failed_save_keeps_active_revision_and_prior_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut manager = SettingsManager::load_from_path(registry(), "test", path.clone());
        let mut initial = SettingsDraft::from_snapshot(&manager.active());
        initial.set(VALUE, 3);
        manager.request_save(initial).unwrap();
        wait(&mut manager);
        let prior = std::fs::read(&path).unwrap();
        let revision = manager.active().revision();

        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        let mut failing = SettingsDraft::from_snapshot(&manager.active());
        failing.set(VALUE, 9);
        manager.request_save(failing).unwrap();
        wait(&mut manager);
        assert_eq!(manager.view().persistence, SettingsPersistenceState::Failed);
        assert_eq!(manager.active().revision(), revision);
        assert_eq!(manager.active().get(VALUE).unwrap(), 3);
        std::fs::remove_dir(&path).unwrap();
        std::fs::write(&path, &prior).unwrap();
        assert_eq!(std::fs::read(path).unwrap(), prior);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn unknown_values_survive_manager_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let document = SettingsDocument {
            writer_version: "old".to_owned(),
            values: std::collections::BTreeMap::from([
                (VALUE.id().to_owned(), serde_json::json!(5)),
                ("future.opaque".to_owned(), serde_json::json!({"x": [1, 2]})),
            ]),
        };
        std::fs::write(&path, encode_settings(&document).unwrap()).unwrap();
        let mut manager = SettingsManager::load_from_path(registry(), "new", path.clone());
        let mut draft = SettingsDraft::from_snapshot(&manager.active());
        draft.set(VALUE, 6);
        manager.request_save(draft).unwrap();
        wait(&mut manager);
        let saved = decode_settings(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(
            saved.values["future.opaque"],
            serde_json::json!({"x": [1, 2]})
        );
    }
}
