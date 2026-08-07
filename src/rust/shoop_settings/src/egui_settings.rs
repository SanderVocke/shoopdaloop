use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

pub const EGUI_SETTINGS_FORMAT: &str = "shoop-egui-settings";
pub const EGUI_SETTINGS_FORMAT_MAJOR: u16 = 1;
pub const EGUI_SETTINGS_FORMAT_MINOR: u16 = 0;
pub const EGUI_SETTINGS_DOCUMENT_VERSION: u16 = 1;
pub const EGUI_SETTINGS_STORAGE_KEY: &str = "org.shoopdaloop.egui.settings";
pub const EGUI_SETTINGS_FILENAME: &str = "settings.json";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SettingsFormatVersion {
    pub major: u16,
    pub minor: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EgSettingsDocument {
    pub writer_version: String,
    pub values: BTreeMap<String, Value>,
}

impl EgSettingsDocument {
    pub fn empty(writer_version: impl Into<String>) -> Self {
        Self {
            writer_version: writer_version.into(),
            values: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum SettingsDocumentError {
    Malformed(String),
    UnsupportedFormat(String),
    UnsupportedFormatVersion { major: u16, minor: u16 },
    UnsupportedDocumentVersion(u16),
    Migration(String),
}

impl fmt::Display for SettingsDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(message) => write!(formatter, "malformed settings document: {message}"),
            Self::UnsupportedFormat(format) => {
                write!(formatter, "unsupported settings format {format:?}")
            }
            Self::UnsupportedFormatVersion { major, minor } => {
                write!(
                    formatter,
                    "unsupported settings format version {major}.{minor}"
                )
            }
            Self::UnsupportedDocumentVersion(version) => {
                write!(formatter, "unsupported settings document version {version}")
            }
            Self::Migration(message) => write!(formatter, "settings migration failed: {message}"),
        }
    }
}

impl Error for SettingsDocumentError {}

#[derive(Deserialize)]
struct SettingsEnvelope {
    format: String,
    format_version: SettingsFormatVersion,
    document_version: u16,
}

#[derive(Deserialize)]
struct SettingsDocumentV1 {
    writer_version: String,
    values: BTreeMap<String, Value>,
}

#[derive(Serialize)]
struct EncodedSettingsDocument<'a> {
    format: &'static str,
    format_version: SettingsFormatVersion,
    document_version: u16,
    writer_version: &'a str,
    values: &'a BTreeMap<String, Value>,
}

struct MigrationStep {
    from: u16,
    to: u16,
    migrate: fn(Value) -> Result<Value, String>,
}

const MIGRATIONS: &[MigrationStep] = &[];

pub fn decode_egui_settings(input: &str) -> Result<EgSettingsDocument, SettingsDocumentError> {
    let mut value: Value = serde_json::from_str(input)
        .map_err(|error| SettingsDocumentError::Malformed(error.to_string()))?;
    let format = value
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("<missing>");
    if format != EGUI_SETTINGS_FORMAT {
        return Err(SettingsDocumentError::UnsupportedFormat(format.to_owned()));
    }
    let envelope: SettingsEnvelope = serde_json::from_value(value.clone())
        .map_err(|error| SettingsDocumentError::Malformed(error.to_string()))?;
    if envelope.format_version.major != EGUI_SETTINGS_FORMAT_MAJOR
        || envelope.format_version.minor > EGUI_SETTINGS_FORMAT_MINOR
    {
        return Err(SettingsDocumentError::UnsupportedFormatVersion {
            major: envelope.format_version.major,
            minor: envelope.format_version.minor,
        });
    }
    value = run_migrations(envelope.document_version, value, MIGRATIONS)?;
    let document: SettingsDocumentV1 = serde_json::from_value(value)
        .map_err(|error| SettingsDocumentError::Malformed(error.to_string()))?;
    Ok(EgSettingsDocument {
        writer_version: document.writer_version,
        values: document.values,
    })
}

pub fn encode_egui_settings(
    document: &EgSettingsDocument,
) -> Result<String, SettingsDocumentError> {
    let encoded = EncodedSettingsDocument {
        format: EGUI_SETTINGS_FORMAT,
        format_version: SettingsFormatVersion {
            major: EGUI_SETTINGS_FORMAT_MAJOR,
            minor: EGUI_SETTINGS_FORMAT_MINOR,
        },
        document_version: EGUI_SETTINGS_DOCUMENT_VERSION,
        writer_version: &document.writer_version,
        values: &document.values,
    };
    let mut output = serde_json::to_string_pretty(&encoded)
        .map_err(|error| SettingsDocumentError::Malformed(error.to_string()))?;
    output.push('\n');
    Ok(output)
}

fn run_migrations(
    mut version: u16,
    mut document: Value,
    migrations: &[MigrationStep],
) -> Result<Value, SettingsDocumentError> {
    if version > EGUI_SETTINGS_DOCUMENT_VERSION {
        return Err(SettingsDocumentError::UnsupportedDocumentVersion(version));
    }
    while version < EGUI_SETTINGS_DOCUMENT_VERSION {
        let step = migrations
            .iter()
            .find(|step| step.from == version && step.to == version.saturating_add(1))
            .ok_or(SettingsDocumentError::UnsupportedDocumentVersion(version))?;
        document = (step.migrate)(document).map_err(SettingsDocumentError::Migration)?;
        version = step.to;
        let object = document.as_object_mut().ok_or_else(|| {
            SettingsDocumentError::Migration("migration returned a non-object".to_owned())
        })?;
        object.insert("document_version".to_owned(), Value::from(version));
    }
    Ok(document)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingValueType {
    Bool,
    U32,
    I32,
    F64,
    String,
}

impl fmt::Display for SettingValueType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool => formatter.write_str("boolean"),
            Self::U32 => formatter.write_str("unsigned integer"),
            Self::I32 => formatter.write_str("signed integer"),
            Self::F64 => formatter.write_str("number"),
            Self::String => formatter.write_str("string"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SettingValue {
    Bool(bool),
    U32(u32),
    I32(i32),
    F64(f64),
    String(String),
}

impl SettingValue {
    pub const fn value_type(&self) -> SettingValueType {
        match self {
            Self::Bool(_) => SettingValueType::Bool,
            Self::U32(_) => SettingValueType::U32,
            Self::I32(_) => SettingValueType::I32,
            Self::F64(_) => SettingValueType::F64,
            Self::String(_) => SettingValueType::String,
        }
    }

    fn to_json(&self) -> Value {
        match self {
            Self::Bool(value) => Value::Bool(*value),
            Self::U32(value) => Value::from(*value),
            Self::I32(value) => Value::from(*value),
            Self::F64(value) => Value::from(*value),
            Self::String(value) => Value::String(value.clone()),
        }
    }
}

mod private {
    pub trait Sealed {}
    impl Sealed for bool {}
    impl Sealed for u32 {}
    impl Sealed for i32 {}
    impl Sealed for f64 {}
    impl Sealed for String {}
}

pub trait SettingType:
    private::Sealed + Clone + fmt::Debug + PartialEq + Send + Sync + 'static
{
    const VALUE_TYPE: SettingValueType;

    fn into_setting_value(self) -> SettingValue;
    fn from_setting_value(value: &SettingValue) -> Option<Self>;
    fn default_editor() -> SettingEditor;
}

macro_rules! setting_type {
    ($rust:ty, $variant:ident, $value_type:ident, $editor:expr) => {
        impl SettingType for $rust {
            const VALUE_TYPE: SettingValueType = SettingValueType::$value_type;

            fn into_setting_value(self) -> SettingValue {
                SettingValue::$variant(self)
            }

            fn from_setting_value(value: &SettingValue) -> Option<Self> {
                match value {
                    SettingValue::$variant(value) => Some(value.clone()),
                    _ => None,
                }
            }

            fn default_editor() -> SettingEditor {
                $editor
            }
        }
    };
}

setting_type!(bool, Bool, Bool, SettingEditor::Checkbox);
setting_type!(
    u32,
    U32,
    U32,
    SettingEditor::UnsignedInteger {
        min: 0,
        max: u32::MAX
    }
);
setting_type!(
    i32,
    I32,
    I32,
    SettingEditor::SignedInteger {
        min: i32::MIN,
        max: i32::MAX
    }
);
setting_type!(
    f64,
    F64,
    F64,
    SettingEditor::Number {
        min: f64::MIN,
        max: f64::MAX
    }
);
setting_type!(String, String, String, SettingEditor::Text);

#[derive(Debug)]
pub struct SettingKey<T> {
    id: &'static str,
    marker: PhantomData<fn() -> T>,
}

impl<T> SettingKey<T> {
    pub const fn new(id: &'static str) -> Self {
        Self {
            id,
            marker: PhantomData,
        }
    }

    pub const fn id(self) -> &'static str {
        self.id
    }
}

impl<T> Clone for SettingKey<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for SettingKey<T> {}

impl<T, U> PartialEq<SettingKey<U>> for SettingKey<T> {
    fn eq(&self, other: &SettingKey<U>) -> bool {
        self.id == other.id
    }
}

impl<T> Eq for SettingKey<T> {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingEffect {
    Immediate,
    NextUse,
    RestartRequired,
}

impl SettingEffect {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Immediate => "Applies immediately",
            Self::NextUse => "Applies the next time this feature is opened",
            Self::RestartRequired => "Requires an application restart",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SettingEditor {
    Checkbox,
    UnsignedInteger { min: u32, max: u32 },
    SignedInteger { min: i32, max: i32 },
    Number { min: f64, max: f64 },
    Text,
}

impl SettingEditor {
    pub const fn value_type(&self) -> SettingValueType {
        match self {
            Self::Checkbox => SettingValueType::Bool,
            Self::UnsignedInteger { .. } => SettingValueType::U32,
            Self::SignedInteger { .. } => SettingValueType::I32,
            Self::Number { .. } => SettingValueType::F64,
            Self::Text => SettingValueType::String,
        }
    }

    fn validate(&self, value: &SettingValue) -> bool {
        match (self, value) {
            (Self::Checkbox, SettingValue::Bool(_)) | (Self::Text, SettingValue::String(_)) => true,
            (Self::UnsignedInteger { min, max }, SettingValue::U32(value)) => {
                value >= min && value <= max
            }
            (Self::SignedInteger { min, max }, SettingValue::I32(value)) => {
                value >= min && value <= max
            }
            (Self::Number { min, max }, SettingValue::F64(value)) => {
                value.is_finite() && value >= min && value <= max
            }
            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SettingDefinition<T: SettingType> {
    key: SettingKey<T>,
    default: T,
    category: &'static str,
    category_order: u32,
    setting_order: u32,
    label: &'static str,
    help: &'static str,
    effect: SettingEffect,
    editor: SettingEditor,
}

impl<T: SettingType> SettingDefinition<T> {
    pub fn new(
        key: SettingKey<T>,
        default: T,
        category: &'static str,
        label: &'static str,
        help: &'static str,
    ) -> Self {
        Self {
            key,
            default,
            category,
            category_order: 0,
            setting_order: 0,
            label,
            help,
            effect: SettingEffect::Immediate,
            editor: T::default_editor(),
        }
    }

    pub const fn category_order(mut self, order: u32) -> Self {
        self.category_order = order;
        self
    }

    pub const fn setting_order(mut self, order: u32) -> Self {
        self.setting_order = order;
        self
    }

    pub const fn effect(mut self, effect: SettingEffect) -> Self {
        self.effect = effect;
        self
    }

    pub fn editor(mut self, editor: SettingEditor) -> Self {
        self.editor = editor;
        self
    }

    fn erase(self) -> ErasedSettingDefinition {
        ErasedSettingDefinition {
            key: self.key.id.to_owned(),
            value_type: T::VALUE_TYPE,
            default: self.default.into_setting_value(),
            category: self.category.to_owned(),
            category_order: self.category_order,
            setting_order: self.setting_order,
            label: self.label.to_owned(),
            help: self.help.to_owned(),
            effect: self.effect,
            editor: self.editor,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ErasedSettingDefinition {
    key: String,
    value_type: SettingValueType,
    default: SettingValue,
    category: String,
    category_order: u32,
    setting_order: u32,
    label: String,
    help: String,
    effect: SettingEffect,
    editor: SettingEditor,
}

impl ErasedSettingDefinition {
    pub fn key(&self) -> &str {
        &self.key
    }

    pub const fn value_type(&self) -> SettingValueType {
        self.value_type
    }

    pub fn default_value(&self) -> &SettingValue {
        &self.default
    }

    pub fn category(&self) -> &str {
        &self.category
    }

    pub const fn category_order(&self) -> u32 {
        self.category_order
    }

    pub const fn setting_order(&self) -> u32 {
        self.setting_order
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn help(&self) -> &str {
        &self.help
    }

    pub const fn effect(&self) -> SettingEffect {
        self.effect
    }

    pub fn editor(&self) -> &SettingEditor {
        &self.editor
    }

    pub fn validate(&self, value: &SettingValue) -> bool {
        value.value_type() == self.value_type && self.editor.validate(value)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum SettingsRegistryError {
    DuplicateKey(String),
    InvalidKey(String),
    EmptyCategory(String),
    EmptyLabel(String),
    EditorTypeMismatch(String),
    InvalidDefault(String),
}

impl fmt::Display for SettingsRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateKey(key) => write!(formatter, "duplicate setting key {key:?}"),
            Self::InvalidKey(key) => write!(formatter, "invalid setting key {key:?}"),
            Self::EmptyCategory(key) => write!(formatter, "setting {key:?} has an empty category"),
            Self::EmptyLabel(key) => write!(formatter, "setting {key:?} has an empty label"),
            Self::EditorTypeMismatch(key) => {
                write!(formatter, "setting {key:?} has an incompatible editor")
            }
            Self::InvalidDefault(key) => {
                write!(formatter, "setting {key:?} has an invalid default")
            }
        }
    }
}

impl Error for SettingsRegistryError {}

#[derive(Default)]
pub struct SettingsRegistryBuilder {
    definitions: Vec<ErasedSettingDefinition>,
    keys: BTreeSet<String>,
}

impl SettingsRegistryBuilder {
    pub fn register<T: SettingType>(
        &mut self,
        definition: SettingDefinition<T>,
    ) -> Result<(), SettingsRegistryError> {
        let definition = definition.erase();
        validate_setting_key(&definition.key)?;
        if self.keys.contains(&definition.key) {
            return Err(SettingsRegistryError::DuplicateKey(definition.key));
        }
        if definition.category.trim().is_empty() {
            return Err(SettingsRegistryError::EmptyCategory(definition.key));
        }
        if definition.label.trim().is_empty() {
            return Err(SettingsRegistryError::EmptyLabel(definition.key));
        }
        if definition.editor.value_type() != definition.value_type {
            return Err(SettingsRegistryError::EditorTypeMismatch(definition.key));
        }
        if !definition.validate(&definition.default) {
            return Err(SettingsRegistryError::InvalidDefault(definition.key));
        }
        self.keys.insert(definition.key.clone());
        self.definitions.push(definition);
        Ok(())
    }

    pub fn finish(mut self) -> SettingsRegistry {
        self.definitions.sort_by(|left, right| {
            (
                left.category_order,
                &left.category,
                left.setting_order,
                &left.key,
            )
                .cmp(&(
                    right.category_order,
                    &right.category,
                    right.setting_order,
                    &right.key,
                ))
        });
        let by_key = self
            .definitions
            .iter()
            .enumerate()
            .map(|(index, definition)| (definition.key.clone(), index))
            .collect();
        SettingsRegistry {
            definitions: Arc::from(self.definitions),
            by_key,
        }
    }
}

fn validate_setting_key(key: &str) -> Result<(), SettingsRegistryError> {
    let valid = !key.is_empty()
        && key.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        });
    if valid {
        Ok(())
    } else {
        Err(SettingsRegistryError::InvalidKey(key.to_owned()))
    }
}

#[derive(Clone, Debug)]
pub struct SettingsRegistry {
    definitions: Arc<[ErasedSettingDefinition]>,
    by_key: BTreeMap<String, usize>,
}

impl SettingsRegistry {
    pub fn definitions(&self) -> &[ErasedSettingDefinition] {
        &self.definitions
    }

    pub fn definition(&self, key: &str) -> Option<&ErasedSettingDefinition> {
        self.by_key
            .get(key)
            .and_then(|index| self.definitions.get(*index))
    }

    pub fn defaults(&self, revision: u64) -> SettingsSnapshot {
        SettingsSnapshot {
            revision,
            values: Arc::new(
                self.definitions
                    .iter()
                    .map(|definition| (definition.key.clone(), definition.default.clone()))
                    .collect(),
            ),
        }
    }

    pub fn resolve(&self, document: &EgSettingsDocument, revision: u64) -> ResolvedSettings {
        let mut values = BTreeMap::new();
        let mut diagnostics = Vec::new();
        for definition in self.definitions.iter() {
            let value = match document.values.get(&definition.key) {
                Some(raw) => match setting_value_from_json(definition.value_type, raw) {
                    Some(value) if definition.validate(&value) => value,
                    _ => {
                        diagnostics.push(SettingsDiagnostic {
                            key: Some(definition.key.clone()),
                            message: format!(
                                "{} has an invalid {}; using its default",
                                definition.label, definition.value_type
                            ),
                        });
                        definition.default.clone()
                    }
                },
                None => definition.default.clone(),
            };
            values.insert(definition.key.clone(), value);
        }
        ResolvedSettings {
            snapshot: SettingsSnapshot {
                revision,
                values: Arc::new(values),
            },
            diagnostics,
        }
    }

    pub fn validate_draft(&self, draft: &SettingsDraft) -> Result<(), SettingsDraftError> {
        for definition in self.definitions.iter() {
            let value = draft
                .values
                .get(&definition.key)
                .ok_or_else(|| SettingsDraftError::MissingValue(definition.key.clone()))?;
            if !definition.validate(value) {
                return Err(SettingsDraftError::InvalidValue(definition.key.clone()));
            }
        }
        if let Some(key) = draft
            .values
            .keys()
            .find(|key| !self.by_key.contains_key(*key))
        {
            return Err(SettingsDraftError::UnknownKey(key.clone()));
        }
        Ok(())
    }

    pub fn document_from_draft(
        &self,
        base: &EgSettingsDocument,
        draft: &SettingsDraft,
        writer_version: impl Into<String>,
    ) -> Result<EgSettingsDocument, SettingsDraftError> {
        self.validate_draft(draft)?;
        let mut document = base.clone();
        document.writer_version = writer_version.into();
        for (key, value) in &draft.values {
            document.values.insert(key.clone(), value.to_json());
        }
        Ok(document)
    }
}

fn setting_value_from_json(value_type: SettingValueType, raw: &Value) -> Option<SettingValue> {
    match value_type {
        SettingValueType::Bool => raw.as_bool().map(SettingValue::Bool),
        SettingValueType::U32 => raw
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .map(SettingValue::U32),
        SettingValueType::I32 => raw
            .as_i64()
            .and_then(|value| i32::try_from(value).ok())
            .map(SettingValue::I32),
        SettingValueType::F64 => raw
            .as_f64()
            .filter(|value| value.is_finite())
            .map(SettingValue::F64),
        SettingValueType::String => raw
            .as_str()
            .map(|value| SettingValue::String(value.to_owned())),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsDiagnostic {
    pub key: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct ResolvedSettings {
    pub snapshot: SettingsSnapshot,
    pub diagnostics: Vec<SettingsDiagnostic>,
}

#[derive(Clone, Debug)]
pub struct SettingsSnapshot {
    revision: u64,
    values: Arc<BTreeMap<String, SettingValue>>,
}

impl SettingsSnapshot {
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn get<T: SettingType>(&self, key: SettingKey<T>) -> Result<T, SettingsAccessError> {
        let value = self
            .values
            .get(key.id)
            .ok_or_else(|| SettingsAccessError::UnknownKey(key.id.to_owned()))?;
        T::from_setting_value(value).ok_or_else(|| SettingsAccessError::TypeMismatch {
            key: key.id.to_owned(),
            expected: T::VALUE_TYPE,
            actual: value.value_type(),
        })
    }

    pub fn value(&self, key: &str) -> Option<&SettingValue> {
        self.values.get(key)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum SettingsAccessError {
    UnknownKey(String),
    TypeMismatch {
        key: String,
        expected: SettingValueType,
        actual: SettingValueType,
    },
}

impl fmt::Display for SettingsAccessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownKey(key) => write!(formatter, "unknown setting key {key:?}"),
            Self::TypeMismatch {
                key,
                expected,
                actual,
            } => write!(
                formatter,
                "setting {key:?} has type {actual}, expected {expected}"
            ),
        }
    }
}

impl Error for SettingsAccessError {}

#[derive(Clone, Debug)]
pub struct SettingsDraft {
    base_revision: u64,
    values: BTreeMap<String, SettingValue>,
}

impl SettingsDraft {
    pub fn from_snapshot(snapshot: &SettingsSnapshot) -> Self {
        Self {
            base_revision: snapshot.revision,
            values: (*snapshot.values).clone(),
        }
    }

    pub const fn base_revision(&self) -> u64 {
        self.base_revision
    }

    pub fn value(&self, key: &str) -> Option<&SettingValue> {
        self.values.get(key)
    }

    pub fn set_value(&mut self, key: impl Into<String>, value: SettingValue) {
        self.values.insert(key.into(), value);
    }

    pub fn reset(&mut self, definition: &ErasedSettingDefinition) {
        self.values
            .insert(definition.key.clone(), definition.default.clone());
    }

    pub fn reset_all(&mut self, registry: &SettingsRegistry) {
        self.values = registry
            .definitions
            .iter()
            .map(|definition| (definition.key.clone(), definition.default.clone()))
            .collect();
    }

    pub fn get<T: SettingType>(&self, key: SettingKey<T>) -> Result<T, SettingsAccessError> {
        let value = self
            .values
            .get(key.id)
            .ok_or_else(|| SettingsAccessError::UnknownKey(key.id.to_owned()))?;
        T::from_setting_value(value).ok_or_else(|| SettingsAccessError::TypeMismatch {
            key: key.id.to_owned(),
            expected: T::VALUE_TYPE,
            actual: value.value_type(),
        })
    }

    pub fn set<T: SettingType>(&mut self, key: SettingKey<T>, value: T) {
        self.values
            .insert(key.id.to_owned(), value.into_setting_value());
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum SettingsDraftError {
    StaleRevision { expected: u64, actual: u64 },
    MissingValue(String),
    UnknownKey(String),
    InvalidValue(String),
}

impl fmt::Display for SettingsDraftError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleRevision { expected, actual } => {
                write!(
                    formatter,
                    "stale settings revision {actual}; current is {expected}"
                )
            }
            Self::MissingValue(key) => write!(formatter, "settings draft is missing {key:?}"),
            Self::UnknownKey(key) => {
                write!(formatter, "settings draft contains unknown key {key:?}")
            }
            Self::InvalidValue(key) => write!(
                formatter,
                "settings draft contains invalid value for {key:?}"
            ),
        }
    }
}

impl Error for SettingsDraftError {}

#[cfg(test)]
mod tests {
    use super::*;

    const COUNT: SettingKey<u32> = SettingKey::new("test.count");
    const ENABLED: SettingKey<bool> = SettingKey::new("test.enabled");

    fn registry() -> SettingsRegistry {
        let mut builder = SettingsRegistryBuilder::default();
        builder
            .register(
                SettingDefinition::new(COUNT, 2, "Test", "Count", "A count")
                    .editor(SettingEditor::UnsignedInteger { min: 0, max: 10 })
                    .setting_order(2),
            )
            .unwrap();
        builder
            .register(
                SettingDefinition::new(ENABLED, false, "Test", "Enabled", "A switch")
                    .setting_order(1),
            )
            .unwrap();
        builder.finish()
    }

    fn current_document(values: Value) -> String {
        serde_json::json!({
            "format": EGUI_SETTINGS_FORMAT,
            "format_version": {"major": 1, "minor": 0},
            "document_version": 1,
            "writer_version": "test",
            "values": values,
        })
        .to_string()
    }

    #[test]
    fn current_document_is_deterministic_and_round_trips() {
        let document = EgSettingsDocument {
            writer_version: "test".to_owned(),
            values: BTreeMap::from([
                ("z.last".to_owned(), Value::Bool(true)),
                ("a.first".to_owned(), Value::from(2)),
            ]),
        };
        let first = encode_egui_settings(&document).unwrap();
        let second = encode_egui_settings(&document).unwrap();
        assert_eq!(first, second);
        assert!(first.find("a.first").unwrap() < first.find("z.last").unwrap());
        assert!(first.ends_with('\n'));
        assert_eq!(decode_egui_settings(&first).unwrap(), document);
    }

    #[test]
    fn format_and_every_version_boundary_are_checked_before_values() {
        let legacy = r#"{"schema":"settings.1","configuration":{}}"#;
        assert_eq!(
            decode_egui_settings(legacy).unwrap_err(),
            SettingsDocumentError::UnsupportedFormat("<missing>".to_owned())
        );

        let mut value: Value =
            serde_json::from_str(&current_document(Value::Object(Default::default()))).unwrap();
        value["format"] = Value::String("other".to_owned());
        assert_eq!(
            decode_egui_settings(&value.to_string()).unwrap_err(),
            SettingsDocumentError::UnsupportedFormat("other".to_owned())
        );
        for (major, minor) in [(0, 0), (2, 0), (1, 1)] {
            value["format"] = Value::String(EGUI_SETTINGS_FORMAT.to_owned());
            value["format_version"] = serde_json::json!({"major": major, "minor": minor});
            assert_eq!(
                decode_egui_settings(&value.to_string()).unwrap_err(),
                SettingsDocumentError::UnsupportedFormatVersion { major, minor }
            );
        }
        value["format_version"] = serde_json::json!({"major": 1, "minor": 0});
        for version in [0, 2] {
            value["document_version"] = Value::from(version);
            assert_eq!(
                decode_egui_settings(&value.to_string()).unwrap_err(),
                SettingsDocumentError::UnsupportedDocumentVersion(version)
            );
        }
    }

    #[test]
    fn migration_dispatch_chains_in_order_and_stops_on_failure() {
        fn first(mut value: Value) -> Result<Value, String> {
            value["values"]["order"] = Value::String("first".to_owned());
            Ok(value)
        }
        let steps = [MigrationStep {
            from: 0,
            to: 1,
            migrate: first,
        }];
        let value = serde_json::json!({"document_version": 0, "values": {}});
        let migrated = run_migrations(0, value, &steps).unwrap();
        assert_eq!(migrated["document_version"], 1);
        assert_eq!(migrated["values"]["order"], "first");

        fn fail(_: Value) -> Result<Value, String> {
            Err("injected".to_owned())
        }
        let failing = [MigrationStep {
            from: 0,
            to: 1,
            migrate: fail,
        }];
        assert_eq!(
            run_migrations(0, serde_json::json!({}), &failing).unwrap_err(),
            SettingsDocumentError::Migration("injected".to_owned())
        );
    }

    #[test]
    fn registration_rejects_duplicates_bad_keys_editors_and_defaults() {
        let mut builder = SettingsRegistryBuilder::default();
        builder
            .register(SettingDefinition::new(COUNT, 2, "Test", "Count", "help"))
            .unwrap();
        assert!(matches!(
            builder.register(SettingDefinition::new(COUNT, 3, "Test", "Other", "help")),
            Err(SettingsRegistryError::DuplicateKey(_))
        ));

        let mut builder = SettingsRegistryBuilder::default();
        assert!(matches!(
            builder.register(SettingDefinition::new(
                SettingKey::<u32>::new("Bad key"),
                2,
                "Test",
                "Count",
                "help"
            )),
            Err(SettingsRegistryError::InvalidKey(_))
        ));
        assert!(matches!(
            builder.register(
                SettingDefinition::new(COUNT, 2, "Test", "Count", "help")
                    .editor(SettingEditor::Checkbox)
            ),
            Err(SettingsRegistryError::EditorTypeMismatch(_))
        ));
        assert!(matches!(
            builder.register(
                SettingDefinition::new(COUNT, 20, "Test", "Count", "help")
                    .editor(SettingEditor::UnsignedInteger { min: 0, max: 10 })
            ),
            Err(SettingsRegistryError::InvalidDefault(_))
        ));
    }

    #[test]
    fn resolution_uses_defaults_warns_for_invalid_and_preserves_unknown() {
        let registry = registry();
        let document = decode_egui_settings(&current_document(serde_json::json!({
            "test.count": 99,
            "unknown.future": {"opaque": [1, 2, 3]}
        })))
        .unwrap();
        let resolved = registry.resolve(&document, 4);
        assert_eq!(resolved.snapshot.get(COUNT).unwrap(), 2);
        assert_eq!(resolved.snapshot.get(ENABLED).unwrap(), false);
        assert_eq!(resolved.snapshot.revision(), 4);
        assert_eq!(resolved.diagnostics.len(), 1);

        let mut draft = SettingsDraft::from_snapshot(&resolved.snapshot);
        draft.set(COUNT, 7);
        let saved = registry
            .document_from_draft(&document, &draft, "new")
            .unwrap();
        assert_eq!(saved.values["test.count"], 7);
        assert_eq!(
            saved.values["unknown.future"],
            serde_json::json!({"opaque": [1, 2, 3]})
        );
    }

    #[test]
    fn typed_reads_and_draft_validation_reject_mismatches_and_stale_shape() {
        let registry = registry();
        let snapshot = registry.defaults(9);
        assert_eq!(snapshot.get(COUNT).unwrap(), 2);
        let wrong = SettingKey::<bool>::new(COUNT.id());
        assert!(matches!(
            snapshot.get(wrong),
            Err(SettingsAccessError::TypeMismatch { .. })
        ));
        let mut draft = SettingsDraft::from_snapshot(&snapshot);
        draft.set_value(COUNT.id(), SettingValue::U32(11));
        assert_eq!(
            registry.validate_draft(&draft),
            Err(SettingsDraftError::InvalidValue(COUNT.id().to_owned()))
        );
        draft.set(COUNT, 5);
        draft.set_value("unknown", SettingValue::Bool(true));
        assert_eq!(
            registry.validate_draft(&draft),
            Err(SettingsDraftError::UnknownKey("unknown".to_owned()))
        );
    }

    #[test]
    fn definition_order_is_stable_and_reset_uses_registered_defaults() {
        let registry = registry();
        assert_eq!(registry.definitions()[0].key(), ENABLED.id());
        assert_eq!(registry.definitions()[1].key(), COUNT.id());
        let snapshot = registry.defaults(1);
        let mut draft = SettingsDraft::from_snapshot(&snapshot);
        draft.set(COUNT, 8);
        draft.reset(registry.definition(COUNT.id()).unwrap());
        assert_eq!(draft.get(COUNT).unwrap(), 2);
        draft.set(ENABLED, true);
        draft.reset_all(&registry);
        assert!(!draft.get(ENABLED).unwrap());
    }
}
