//! Physical fixture conversion; allocation and sample execution remain in production/RTL.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
pub fn reblock(media: &Value, bc: u64, capacity: u64) -> Value {
    if media["block_chunks"].as_u64().unwrap_or(1) == bc {
        let mut m = media.clone();
        m["chunk_limit"] = json!(capacity);
        return m;
    }
    assert_eq!(media["block_chunks"].as_u64().unwrap_or(1), 1);
    let cf = media["chunk_frames"].as_u64().unwrap();
    let raw: Vec<i32> = serde_json::from_value(media["samples"].clone()).unwrap();
    let mut chunks = BTreeSet::new();
    let mut following = BTreeMap::new();
    let mut previous = BTreeMap::new();
    for view in media["views"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| !v.is_null())
    {
        let ids: Vec<u64> = serde_json::from_value(view["chunks"].clone()).unwrap();
        chunks.extend(&ids);
        for pair in ids.windows(2) {
            if let Some(old) = following.insert(pair[0], pair[1]) {
                assert_eq!(old, pair[1], "branching fixture chunks");
            }
            if let Some(old) = previous.insert(pair[1], pair[0]) {
                assert_eq!(old, pair[0], "merging fixture chunks");
            }
        }
    }
    let mut mapping = BTreeMap::new();
    let mut samples = Vec::new();
    for head in chunks.iter().filter(|id| !previous.contains_key(id)) {
        let mut sequence = vec![*head];
        while let Some(next) = following.get(sequence.last().unwrap()) {
            sequence.push(*next);
            assert!(sequence.len() <= chunks.len(), "cyclic fixture");
        }
        let start = samples.len() / cf as usize;
        samples.resize(
            samples.len()
                + (sequence.len() as u64).div_ceil(bc) as usize * bc as usize * cf as usize,
            0,
        );
        for (offset, old) in sequence.into_iter().enumerate() {
            let new = start + offset;
            mapping.insert(old, new as u64);
            samples[new * cf as usize..(new + 1) * cf as usize]
                .copy_from_slice(&raw[(old * cf) as usize..((old + 1) * cf) as usize]);
        }
    }
    assert_eq!(mapping.len(), chunks.len(), "cyclic fixture");
    let views: Vec<_> = media["views"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            if v.is_null() {
                return Value::Null;
            }
            let mut v = v.clone();
            v["chunks"] = json!(
                v["chunks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|id| mapping[&id.as_u64().unwrap()])
                    .collect::<Vec<_>>()
            );
            v
        })
        .collect();
    let mut free = Vec::new();
    for _ in 0..(media["free_chunks"].as_array().unwrap().len() as u64).div_ceil(bc) {
        let first = samples.len() as u64 / cf;
        free.extend(first..first + bc);
        samples.resize(samples.len() + (bc * cf) as usize, 0);
    }
    assert!(samples.len() as u64 / cf <= capacity);
    let mut result = json!({"block_chunks":bc,"chunk_frames":cf,"chunk_limit":capacity,"samples":samples,"free_chunks":free,"views":views});
    crate::drivers::store::canonicalize(&mut result);
    result
}
pub fn logical_media(snapshot: &Value, bindings: &Value, states: &[Value]) -> Vec<Value> {
    let mut result = Vec::new();
    let cf = snapshot["chunk_frames"].as_u64().unwrap();
    let raw = snapshot["samples"].as_array().unwrap();
    for binding in bindings.as_array().unwrap() {
        let state = states.iter().find(|s| s["id"] == binding["id"]).unwrap();
        if state["mode"] == "recording" {
            continue;
        }
        let channels: Vec<Vec<Value>> = binding["views"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| {
                let view = &snapshot["views"][id.as_u64().unwrap() as usize];
                let data: Vec<_> = view["chunks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|chunk| {
                        let first = (chunk.as_u64().unwrap() * cf) as usize;
                        raw[first..first + cf as usize].iter().cloned()
                    })
                    .collect();
                let offset = view["range"]["offset"].as_u64().unwrap_or(0) as usize;
                data[offset..offset + view["frames"].as_u64().unwrap() as usize].to_vec()
            })
            .collect();
        if let Some(first) = channels.first().filter(|c| !c.is_empty()) {
            result.push(json!({"name":format!("loop:{}:{}",binding["id"]["slot"],binding["id"]["generation"]),"frames":first.len(),"samples":{"encoding":"q824","channels":channels}}));
        }
    }
    result
}
