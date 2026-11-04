//! A live import retimes only its automatic grid correction through sync descendants.
use super::*;
use shoop_app_protocol::SessionSnapshot;
use std::collections::BTreeMap;

pub struct FamilyPlan {
    pub format: ConversionTarget,
    pub root: LoopId,
    pub members: Vec<image::FamilyMember>,
    pub changed: Vec<LoopId>,
}

fn reaches(
    mut id: LoopId,
    root: LoopId,
    sources: &BTreeMap<LoopId, LoopId>,
    count: usize,
) -> Result<bool> {
    for _ in 0..=count {
        if id == root {
            return Ok(true);
        }
        match sources.get(&id) {
            Some(source) if *source != id => id = *source,
            _ => return Ok(false),
        }
    }
    anyhow::bail!("cyclic sync dependencies")
}

fn timing(
    format: ConversionTarget,
    root: LoopId,
    members: &[image::FamilyMember],
    frames: u64,
    rate: u32,
) -> Result<ConversionPlan> {
    Ok(ConversionPlan::new(
        format,
        &members
            .iter()
            .map(|member| LoopTiming {
                id: member.loop_id.raw(),
                sync_source: member.sync_source.map(|id| id.raw()),
                frames: if member.loop_id == root {
                    frames
                } else {
                    member.expected_frames
                },
                sample_rate: if member.loop_id == root {
                    rate
                } else {
                    format.sample_rate
                },
            })
            .collect::<Vec<_>>(),
    )?)
}

impl FamilyPlan {
    pub fn new(
        state: &SessionSnapshot,
        root: LoopId,
        frames: u64,
        rate: u32,
    ) -> Result<Option<Self>> {
        let format = state
            .status
            .conversion_target()
            .ok_or_else(|| anyhow!("engine format unavailable"))?;
        ensure!(rate > 0, "invalid source rate");
        let single = ConversionPlan::new(
            format,
            &[LoopTiming {
                id: root.raw(),
                sync_source: None,
                frames,
                sample_rate: rate,
            }],
        )?;
        let automatic = single.loops[0].scale.multiply(FrameScale::new(
            u128::from(rate),
            u128::from(format.sample_rate),
        )?)?;
        if frames == 0 || automatic.is_identity() {
            return Ok(None);
        }
        let loops = state
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .map(|entry| (entry.id, entry))
            .collect::<BTreeMap<_, _>>();
        ensure!(loops.contains_key(&root), "stale import root");
        let sources = state
            .sync_sources
            .iter()
            .copied()
            .collect::<BTreeMap<_, _>>();
        let mut members = Vec::new();
        for (id, entry) in &loops {
            let descendant = reaches(*id, root, &sources, loops.len())?;
            if !descendant && !reaches(root, *id, &sources, loops.len())? {
                continue;
            }
            ensure!(
                entry.composite_kind == CompositeKind::None
                    && entry.structural_state == StructuralState::Confirmed,
                "family contains unsupported or unconfirmed loops"
            );
            members.push(image::FamilyMember {
                loop_id: *id,
                sync_source: sources.get(id).copied(),
                expected_revision: entry.content_revision,
                expected_frames: entry.length_frames,
                fence_content: descendant,
                restart: *id == root,
                content: None,
            });
        }
        let plan = timing(format, root, &members, frames, rate)?;
        let changed = plan
            .loops
            .iter()
            .filter_map(|item| {
                let id = LoopId::from_raw(item.id);
                (id == root || item.frames != loops[&id].length_frames).then_some(id)
            })
            .collect::<Vec<_>>();
        for id in &changed {
            let entry = loops[id];
            ensure!(
                !matches!(
                    entry.mode,
                    LoopMode::Recording | LoopMode::Replacing | LoopMode::RecordingDryIntoWet
                ) && !matches!(
                    entry.next_mode,
                    LoopMode::Recording | LoopMode::Replacing | LoopMode::RecordingDryIntoWet
                ),
                "stop affected recording before importing"
            );
        }
        Ok(Some(Self {
            format,
            root,
            members,
            changed,
        }))
    }

    pub fn current(&self, state: &SessionSnapshot) -> Result<()> {
        ensure!(
            state.status.conversion_target() == Some(self.format),
            "engine format changed"
        );
        let loops = state
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .map(|entry| (entry.id, entry))
            .collect::<BTreeMap<_, _>>();
        let sources = state
            .sync_sources
            .iter()
            .copied()
            .collect::<BTreeMap<_, _>>();
        let ids = self
            .members
            .iter()
            .map(|member| member.loop_id)
            .collect::<BTreeSet<_>>();
        for id in loops.keys() {
            ensure!(
                !reaches(*id, self.root, &sources, loops.len())? || ids.contains(id),
                "sync family changed during import"
            );
        }
        for member in &self.members {
            let entry = loops
                .get(&member.loop_id)
                .ok_or_else(|| anyhow!("family member removed during import"))?;
            ensure!(
                sources
                    .get(&member.loop_id)
                    .copied()
                    .unwrap_or(member.loop_id)
                    == member.sync_source.unwrap_or(member.loop_id)
                    && (!member.fence_content
                        || (entry.content_revision == member.expected_revision
                            && entry.length_frames == member.expected_frames)),
                "family content or dependency changed during import"
            );
        }
        Ok(())
    }
}

fn map_channel(channel: &mut image::Channel, scale: FrameScale) -> Result<MediaTiming> {
    let origin = channel
        .start_offset
        .checked_add(channel.capture_alignment)
        .ok_or_else(|| anyhow!("playback origin overflow"))?;
    let map = MediaTiming::new(scale, origin)?;
    channel.start_offset = i64::try_from(scale.signed_ceil(i128::from(channel.start_offset))?)?;
    channel.capture_alignment = map
        .target_origin
        .checked_sub(channel.start_offset)
        .ok_or_else(|| anyhow!("capture alignment overflow"))?;
    channel.preplay = scale.ceil(channel.preplay)?;
    Ok(map)
}

fn audio(
    channel: &mut image::Channel,
    base: &image::Image,
    output: &mut Vec<image::Payload>,
    scale: FrameScale,
    period: u64,
    bytes: &mut u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<()> {
    let periodic = channel.start_offset == 0
        && channel.capture_alignment == 0
        && channel.frames == period
        && period > 0;
    let map = map_channel(channel, scale)?;
    let Some(object) = channel.object else {
        return Ok(());
    };
    let image::Payload::Audio(samples) = &base.payloads[object as usize - 1] else {
        anyhow::bail!("missing family audio");
    };
    let length = map.length(channel.frames)?;
    let size = length
        .checked_mul(4)
        .ok_or_else(|| anyhow!("converted audio size overflow"))?;
    ensure!(
        size <= MAX_SESSION_MEDIA_BYTES.saturating_sub(*bytes),
        "converted family exceeds media budget"
    );
    let samples = if scale.is_identity() {
        Arc::clone(samples)
    } else {
        let mut work = AudioConversion::new(samples, map, periodic, length)?;
        while !work.step(4096)? {
            ensure!(!cancelled(), "family conversion cancelled");
        }
        Arc::new(work.finish()?)
    };
    *bytes += size;
    channel.frames = length;
    output.push(image::Payload::Audio(samples));
    channel.object = Some(output.len() as u32);
    Ok(())
}

fn midi(
    channel: &mut image::MidiChannel,
    base: &image::Image,
    output: &mut Vec<image::Payload>,
    scale: FrameScale,
    rate: u32,
    bytes: &mut u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<()> {
    let map = map_channel(&mut channel.channel, scale)?;
    let Some(object) = channel.channel.object else {
        channel.channel.frames = map.length(channel.channel.frames)?;
        return Ok(());
    };
    let image::Payload::Midi(events) = &base.payloads[object as usize - 1] else {
        anyhow::bail!("missing family MIDI");
    };
    let converted = if scale.is_identity() {
        Arc::clone(events)
    } else {
        let source = ExactMidi {
            sample_rate: rate,
            length_frames: channel.channel.frames,
            start_state: channel.initial.clone(),
            events: events
                .iter()
                .enumerate()
                .map(|(index, event)| ExactMidiEvent {
                    frame: u64::from(event.frame),
                    order: index as u32,
                    data: event.data.to_vec(),
                })
                .collect(),
        };
        let converted = convert_midi_with_cancel(&source, map, rate, cancelled)?;
        Arc::new(
            converted
                .events
                .into_iter()
                .map(|event| {
                    Ok(MidiEventState {
                        frame: u32::try_from(event.frame)?,
                        data: event.data.into(),
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        )
    };
    channel.channel.frames = map.length(channel.channel.frames)?;
    let payload = image::Payload::Midi(converted);
    let size = payload
        .bytes()
        .ok_or_else(|| anyhow!("MIDI size overflow"))?;
    ensure!(
        size <= MAX_SESSION_MEDIA_BYTES.saturating_sub(*bytes),
        "converted family exceeds media budget"
    );
    *bytes += size;
    output.push(payload);
    channel.channel.object = Some(output.len() as u32);
    Ok(())
}

pub(super) fn convert(
    decoded: Decoded,
    base: Arc<image::Image>,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<Arc<image::Image>> {
    base.validate()
        .map_err(|reason| anyhow!("invalid family capture: {reason:?}"))?;
    let image::Manifest::FamilyUpdate {
        format,
        root,
        members,
    } = &base.manifest
    else {
        anyhow::bail!("expected family capture");
    };
    let frames = decoded
        .frames()
        .ok_or_else(|| anyhow!("expected standalone media"))?;
    let plan = timing(*format, *root, members, frames, decoded.rate())?;
    let maps = plan
        .loops
        .iter()
        .map(|item| (LoopId::from_raw(item.id), item))
        .collect::<BTreeMap<_, _>>();
    let automatic = maps[root].scale.multiply(FrameScale::new(
        u128::from(decoded.rate()),
        u128::from(format.sample_rate),
    )?)?;
    let importing_audio = matches!(decoded, Decoded::Audio(_));
    let Value::Decoded(converted) = (Work::Convert {
        decoded,
        target: *format,
        update_length: true,
    })
    .run_with_cancel(&mut *cancelled)?
    else {
        anyhow::bail!("unexpected media conversion result");
    };
    let mapping = converted
        .audio_labels()
        .map(|labels| (0..labels.len() as u32).collect())
        .unwrap_or_default();
    let Value::Image(imported) = (Work::Prepare {
        decoded: converted,
        mapping,
        update_length: true,
    })
    .run_with_cancel(&mut *cancelled)?
    else {
        anyhow::bail!("unexpected import preparation result");
    };
    let image::Manifest::Update {
        frames: Some(imported_frames),
        audio: imported_audio,
        midi: imported_midi,
        ..
    } = &imported.manifest
    else {
        anyhow::bail!("expected media update");
    };
    ensure!(
        *imported_frames == maps[root].frames,
        "inconsistent root duration"
    );
    let mut output = Vec::new();
    let mut bytes = 0;
    let mut members = members.clone();
    for member in &mut members {
        ensure!(!cancelled(), "family conversion cancelled");
        let Some(content) = &mut member.content else {
            continue;
        };
        ensure!(
            content.frames == member.expected_frames,
            "stale captured period"
        );
        let scale = if member.loop_id == *root {
            automatic
        } else {
            maps[&member.loop_id].scale
        };
        for (index, channel) in content.audio.iter_mut().enumerate() {
            if member.loop_id == *root && importing_audio {
                let gain = channel.gain;
                let replacement = imported_audio
                    .iter()
                    .find(|update| update.channel as usize == index)
                    .ok_or_else(|| anyhow!("missing mapped audio channel"))?;
                *channel = replacement.content.clone();
                channel.gain = gain;
                audio(
                    channel,
                    &imported,
                    &mut output,
                    FrameScale::IDENTITY,
                    *imported_frames,
                    &mut bytes,
                    cancelled,
                )?;
            } else {
                audio(
                    channel,
                    &base,
                    &mut output,
                    scale,
                    member.expected_frames,
                    &mut bytes,
                    cancelled,
                )?;
            }
        }
        for (index, channel) in content.midi.iter_mut().enumerate() {
            if member.loop_id == *root && !importing_audio {
                *channel = imported_midi
                    .iter()
                    .find(|update| update.channel as usize == index)
                    .ok_or_else(|| anyhow!("missing imported MIDI channel"))?
                    .content
                    .clone();
                midi(
                    channel,
                    &imported,
                    &mut output,
                    FrameScale::IDENTITY,
                    format.sample_rate,
                    &mut bytes,
                    cancelled,
                )?;
            } else {
                midi(
                    channel,
                    &base,
                    &mut output,
                    scale,
                    format.sample_rate,
                    &mut bytes,
                    cancelled,
                )?;
            }
        }
        content.frames = maps[&member.loop_id].frames;
    }
    let result = Arc::new(image::Image {
        manifest: image::Manifest::FamilyUpdate {
            format: *format,
            root: *root,
            members,
        },
        payloads: output,
    });
    result
        .validate()
        .map_err(|reason| anyhow!("invalid converted family: {reason:?}"))?;
    validate_initials(&result)?;
    Ok(result)
}
