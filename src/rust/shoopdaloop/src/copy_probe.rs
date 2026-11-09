//! Shared assertions for copies persisted by the native and browser canaries.
pub fn verify(bundle: &shoop_session::SessionBundle) -> Result<(), String> {
    let track = bundle
        .document
        .track_groups
        .iter()
        .flat_map(|g| &g.tracks)
        .nth(1)
        .ok_or("copy canary track missing")?;
    let source = &track.loops[1];
    let target = &track.loops[6];
    if source.length_frames != target.length_frames
        || source.channels.len() != target.channels.len()
        || source.name != target.name
        || source.execution != target.execution
    {
        return Err("primitive copy metadata changed".into());
    }
    for (a, b) in source.channels.iter().zip(&target.channels) {
        let mut expected = a.clone();
        expected.id = b.id;
        expected.media_id.clone_from(&b.media_id);
        if expected != *b
            || a.media_id.as_ref().and_then(|id| bundle.media.get(id))
                != b.media_id.as_ref().and_then(|id| bundle.media.get(id))
        {
            return Err("primitive copy lost exact channel media or timing".into());
        }
    }
    let source = track.loops[3]
        .composite
        .as_ref()
        .ok_or("composite source missing")?;
    let mut copied = track.loops[7]
        .composite
        .clone()
        .ok_or("composite copy missing")?;
    if source.instances.len() != copied.instances.len() {
        return Err("composite copy event count changed".into());
    }
    let mut ids = std::collections::BTreeMap::new();
    for (a, b) in source.instances.iter().zip(&mut copied.instances) {
        if a.instance_id == b.instance_id {
            return Err("composite copy reused an event identity".into());
        }
        ids.insert(b.instance_id, a.instance_id);
        b.instance_id = a.instance_id;
    }
    for event in &mut copied
        .timing
        .as_mut()
        .ok_or("copied geometry missing")?
        .instances
    {
        event.instance_id = *ids
            .get(&event.instance_id)
            .ok_or("copied geometry identity missing")?;
    }
    if source != &copied {
        return Err("composite copy lost definition, child references or frozen geometry".into());
    }
    Ok(())
}
