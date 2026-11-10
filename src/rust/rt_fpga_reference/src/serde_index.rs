//! Transport indices have a checked, platform-independent 32-bit representation.
use serde::{de::Error as _, ser::Error as _, Deserialize, Deserializer, Serializer};

pub fn serialize<S: Serializer>(value: &usize, serializer: S) -> Result<S::Ok, S::Error> {
    let value = u32::try_from(*value).map_err(S::Error::custom)?;
    serializer.serialize_u32(value)
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<usize, D::Error> {
    usize::try_from(u32::deserialize(deserializer)?).map_err(D::Error::custom)
}

pub mod optional {
    use super::*;
    use serde::Serialize;

    pub fn serialize<S: Serializer>(
        value: &Option<usize>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value
            .map(u32::try_from)
            .transpose()
            .map_err(S::Error::custom)?
            .serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<usize>, D::Error> {
        Option::<u32>::deserialize(deserializer)?
            .map(usize::try_from)
            .transpose()
            .map_err(D::Error::custom)
    }
}
