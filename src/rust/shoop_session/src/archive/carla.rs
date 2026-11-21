//! Archive members for opaque Carla state, keyed by document owner.

use super::*;
use shoop_app_api::MAX_OPAQUE_STATE_ENVELOPE_BYTES;

pub(super) const DOCUMENT_VERSION: u16 = 15;
// Keep ordinary legacy projects readable by the existing session format.
const INLINE_CHECKPOINT_BYTES: u64 = 64 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Owner {
    Track(u64),
    Bus(u64),
    Take(u64),
}

impl Owner {
    fn path(self) -> String {
        let (category, id) = match self {
            Self::Track(id) => ("track", id),
            Self::Bus(id) => ("bus", id),
            Self::Take(id) => ("take", id),
        };
        format!("carla/{category}-{id}.state")
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct Record {
    owner: Owner,
    pub path: String,
    bytes: u64,
    sha256: String,
}

fn is_carla(kind: FxChainTypeDocument) -> bool {
    matches!(
        kind,
        FxChainTypeDocument::CarlaRack
            | FxChainTypeDocument::CarlaPatchbay
            | FxChainTypeDocument::CarlaPatchbay16x
            | FxChainTypeDocument::RemoteCarlaRack
            | FxChainTypeDocument::RemoteCarlaPatchbay
            | FxChainTypeDocument::RemoteCarlaPatchbay16x
    )
}

fn state_fields(document: &mut SessionDocument) -> impl Iterator<Item = (Owner, &mut String)> {
    let tracks = document
        .track_groups
        .iter_mut()
        .flat_map(|group| &mut group.tracks)
        .filter_map(|track| {
            let chain = track
                .fx_chain
                .as_mut()
                .filter(|chain| is_carla(chain.chain_type))?;
            Some((Owner::Track(track.id), &mut chain.internal_state))
        });
    let buses = document.buses.iter_mut().filter_map(|bus| {
        let chain = bus
            .fx_chain
            .as_mut()
            .filter(|chain| is_carla(chain.chain_type))?;
        Some((Owner::Bus(bus.id), &mut chain.internal_state))
    });
    let takes = document
        .fx_states
        .iter_mut()
        .filter(|state| is_carla(state.chain_type))
        .map(|state| (Owner::Take(state.id), &mut state.internal_state));
    tracks.chain(buses).chain(takes)
}

pub(super) fn encode(
    document: &mut SessionDocument,
    payloads: &mut BTreeMap<String, Vec<u8>>,
    remaining: &mut u64,
    mut budget: Option<&mut Budget>,
) -> Result<Vec<Record>, SessionError> {
    let mut total = 0u64;
    for (_, state) in state_fields(document) {
        let bytes = state.len() as u64;
        if bytes > MAX_OPAQUE_STATE_ENVELOPE_BYTES as u64 {
            return Err(SessionError::ResourceLimit(
                "Carla checkpoint exceeds its size limit".into(),
            ));
        }
        total = total
            .checked_add(bytes)
            .ok_or_else(|| SessionError::ResourceLimit("Carla checkpoint size overflow".into()))?;
    }
    if total <= INLINE_CHECKPOINT_BYTES {
        return Ok(Vec::new());
    }
    let mut records = Vec::new();
    for (owner, state) in state_fields(document) {
        let bytes = state.len() as u64;
        if let Some(budget) = budget.as_deref_mut() {
            budget.charge(bytes)?;
        }
        *remaining = remaining
            .checked_sub(bytes)
            .ok_or_else(|| SessionError::ResourceLimit("encoded payload budget exceeded".into()))?;
        let path = owner.path();
        let state = core::mem::take(state).into_bytes();
        let sha256 = sha256(&state);
        if payloads.insert(path.clone(), state).is_some() {
            return Err(SessionError::Validation(
                "duplicate Carla checkpoint owner".into(),
            ));
        }
        records.push(Record {
            owner,
            path,
            bytes,
            sha256,
        });
    }
    records.sort_by_key(|record| record.owner);
    Ok(records)
}

pub(super) fn decode(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    document: &mut SessionDocument,
    records: Vec<Record>,
    mut budget: Option<&mut Budget>,
) -> Result<(), SessionError> {
    let mut by_owner = BTreeMap::new();
    for record in records {
        if record.path != record.owner.path() || by_owner.insert(record.owner, record).is_some() {
            return Err(SessionError::Validation(
                "invalid or duplicate Carla checkpoint path".into(),
            ));
        }
    }
    for (owner, state) in state_fields(document) {
        let record = by_owner
            .remove(&owner)
            .ok_or_else(|| SessionError::Validation("missing Carla checkpoint".into()))?;
        if !state.is_empty() {
            return Err(SessionError::Validation(
                "Carla checkpoint also appears inline".into(),
            ));
        }
        if record.bytes > MAX_OPAQUE_STATE_ENVELOPE_BYTES as u64 {
            return Err(SessionError::ResourceLimit(
                "Carla checkpoint exceeds its size limit".into(),
            ));
        }
        if let Some(budget) = budget.as_deref_mut() {
            budget.charge(record.bytes)?;
        }
        let bytes = read_entry(archive, &record.path, record.bytes)?;
        if bytes.len() as u64 != record.bytes || sha256(&bytes) != record.sha256 {
            return Err(SessionError::HashMismatch { id: record.path });
        }
        *state = String::from_utf8(bytes)
            .map_err(|_| SessionError::Validation("Carla checkpoint is not UTF-8".into()))?;
    }
    if !by_owner.is_empty() {
        return Err(SessionError::Validation(
            "unreferenced Carla checkpoint".into(),
        ));
    }
    Ok(())
}
