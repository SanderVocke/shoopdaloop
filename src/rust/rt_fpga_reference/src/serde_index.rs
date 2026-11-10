//! Transport indices have a checked, platform-independent 32-bit representation.
use serde::{de::Error as _, ser::Error as _, Deserialize, Deserializer, Serializer};

pub fn serialize<S: Serializer>(value: &usize, serializer: S) -> Result<S::Ok, S::Error> {
    let value = u32::try_from(*value).map_err(S::Error::custom)?;
    serializer.serialize_u32(value)
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<usize, D::Error> {
    usize::try_from(u32::deserialize(deserializer)?).map_err(D::Error::custom)
}
