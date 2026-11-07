use anyhow::{anyhow, ensure, Result};
use shoop_app_api::{ConversionTarget, HostResourceLimits};
use shoop_app_protocol::persistence_image as image;
use shoop_format_conversion::{
    convert_audio, convert_midi, ConversionPlan, ExactMidi, ExactMidiEvent, LoopTiming, MediaTiming,
};
use std::sync::Arc;

fn convert_composite(
    content: &mut shoop_app_protocol::composite::Content,
    target: ConversionTarget,
) -> Result<()> {
    use shoop_format_conversion::{
        CompositeEffectiveInstanceDocument, CompositeSpanDocument, CompositeTimingDocument,
    };
    let p = &content.prepared;
    let timing = CompositeTimingDocument {
        basis_sample_rate: content.document.basis.sample_rate,
        basis_unit_frames: content.document.basis.unit_frames,
        sample_rate: p.sample_rate,
        quantum_frames: p.quantum,
        unit_frames: p.unit_frames,
        iterations: p.iterations,
        instances: p
            .instances
            .iter()
            .map(|i| CompositeEffectiveInstanceDocument {
                instance_id: i.instance_id,
                start_frame: i.start_frame,
                end_frame: i.end_frame,
            })
            .collect(),
        spans: p
            .spans
            .iter()
            .map(|s| {
                Ok(CompositeSpanDocument {
                    target: s.target.raw(),
                    start: s.start,
                    end: s.end,
                    occurrence_start: s.occurrence_start,
                    ordinal: s.ordinal,
                    mode: serde_json::to_string(&s.mode)?,
                })
            })
            .collect::<Result<_>>()?,
    }
    .converted(target)?;
    content.prepared = shoop_app_protocol::composite::Prepared {
        sample_rate: timing.sample_rate,
        quantum: timing.quantum_frames,
        unit_frames: timing.unit_frames,
        iterations: timing.iterations,
        instances: timing
            .instances
            .into_iter()
            .map(|i| shoop_app_protocol::composite::EffectiveInstance {
                instance_id: i.instance_id,
                start_frame: i.start_frame,
                end_frame: i.end_frame,
            })
            .collect(),
        spans: timing
            .spans
            .into_iter()
            .map(|s| {
                Ok(shoop_app_protocol::composite::Span {
                    target: shoop_app_api::LoopId::from_raw(s.target),
                    start: s.start,
                    end: s.end,
                    occurrence_start: s.occurrence_start,
                    ordinal: s.ordinal,
                    mode: serde_json::from_str(&s.mode)?,
                })
            })
            .collect::<Result<_>>()?,
    };
    Ok(())
}

pub(super) fn convert(
    source: Arc<image::Image>,
    target: ConversionTarget,
    limits: HostResourceLimits,
) -> Result<Arc<image::Image>> {
    let bytes = source
        .validate()
        .map_err(|e| anyhow!("invalid driver capture: {e:?}"))?;
    ensure!(
        bytes <= limits.media_bytes,
        "driver capture exceeds the host media budget"
    );
    ensure!(
        source.payloads.len() < limits.objects as usize,
        "driver capture exceeds its object budget"
    );
    ensure!(
        shoop_app_protocol::codec::encode_manifest(&source.manifest)
            .map_err(|e| anyhow!("invalid capture metadata: {e:?}"))?
            .len() as u64
            <= limits.metadata_bytes,
        "driver capture exceeds its metadata budget"
    );
    let image::Manifest::Session(original) = &source.manifest else {
        return Err(anyhow!("driver conversion requires a session"));
    };
    let timings = original
        .tracks
        .iter()
        .flat_map(|t| &t.loops)
        .map(|l| LoopTiming {
            id: l.id.raw(),
            sync_source: if l.composite.is_some() {
                None
            } else {
                l.sync_source.map(|id| id.raw())
            },
            frames: l.frames,
            sample_rate: original.sample_rate,
        })
        .collect::<Vec<_>>();
    let plan = ConversionPlan::new(target, &timings)?;
    let mut result = (*source).clone();
    let image::Manifest::Session(session) = &mut result.manifest else {
        unreachable!()
    };
    session.sample_rate = target.sample_rate;
    let mut charge = 0_u64;
    let mut midi_events = 0_usize;
    let mut maps = Vec::new();
    for loop_ in &mut session.tracks.iter_mut().flat_map(|t| &mut t.loops) {
        let conversion = plan
            .get(loop_.id.raw())
            .ok_or_else(|| anyhow!("missing loop conversion"))?;
        let old_frames = loop_.frames;
        loop_.frames = conversion.frames;
        if let Some(composite) = &mut loop_.composite {
            convert_composite(composite, target)?;
            ensure!(
                composite
                    .prepared
                    .unit_frames
                    .checked_mul(composite.prepared.iterations)
                    == Some(loop_.frames),
                "composite conversion disagrees with loop duration"
            );
        }
        for (midi, channel) in loop_
            .audio
            .iter_mut()
            .map(|c| (false, c))
            .chain(loop_.midi.iter_mut().map(|c| (true, &mut c.channel)))
        {
            let origin = channel
                .start_offset
                .checked_add(channel.capture_alignment)
                .ok_or_else(|| anyhow!("media origin overflow"))?;
            let timing = MediaTiming::new(conversion.scale, origin)?;
            let alignment = i64::try_from(
                conversion
                    .scale
                    .signed_floor(i128::from(channel.capture_alignment))?,
            )?;
            channel.capture_alignment = alignment;
            channel.start_offset = timing
                .target_origin
                .checked_sub(alignment)
                .ok_or_else(|| anyhow!("media offset overflow"))?;
            channel.preplay = conversion.scale.ceil(channel.preplay)?;
            channel.frames = timing.length(channel.frames)?;
            if let Some(object) = channel.object {
                let payload = source
                    .payloads
                    .get(object as usize - 1)
                    .ok_or_else(|| anyhow!("missing conversion payload"))?;
                if let image::Payload::Midi(events) = payload {
                    midi_events = midi_events
                        .checked_add(events.len())
                        .ok_or_else(|| anyhow!("MIDI event capacity overflow"))?;
                    ensure!(
                        midi_events <= limits.midi_events,
                        "converted MIDI exceeds its event budget"
                    );
                }
                let required = match payload {
                    image::Payload::Audio(samples) => {
                        timing.length(samples.len() as u64)?.checked_mul(4)
                    }
                    image::Payload::Midi(_) => payload.bytes(),
                }
                .ok_or_else(|| anyhow!("conversion capacity overflow"))?;
                charge = charge
                    .checked_add(required)
                    .ok_or_else(|| anyhow!("conversion capacity overflow"))?;
                ensure!(
                    charge <= limits.media_bytes
                        && bytes
                            .checked_add(charge)
                            .is_some_and(|n| n <= limits.media_bytes.saturating_mul(2)),
                    "source and converted session exceed driver conversion budget"
                );
                ensure!(
                    maps.len() + 1 < limits.objects as usize,
                    "converted session exceeds its object budget"
                );
                let source_frames = original
                    .tracks
                    .iter()
                    .flat_map(|t| &t.loops)
                    .flat_map(|l| &l.midi)
                    .find(|c| c.channel.object == Some(object))
                    .map_or(0, |c| c.channel.frames);
                maps.push((object, timing, !midi && origin == 0 && old_frames > 0 && matches!(payload, image::Payload::Audio(samples) if samples.len() as u64 == old_frames), source_frames));
                channel.object = Some(maps.len() as u32);
            }
        }
    }
    result.payloads = Vec::with_capacity(maps.len());
    for (object, timing, periodic, source_frames) in maps {
        let payload = match &source.payloads[object as usize - 1] {
            image::Payload::Audio(samples) if timing.scale.is_identity() => {
                image::Payload::Audio(samples.clone())
            }
            image::Payload::Audio(samples) => image::Payload::Audio(Arc::new(convert_audio(
                samples,
                timing,
                periodic,
                limits.media_bytes / 4,
            )?)),
            image::Payload::Midi(events) => {
                let input = ExactMidi {
                    sample_rate: original.sample_rate,
                    length_frames: source_frames,
                    start_state: Vec::new(),
                    events: events
                        .iter()
                        .map(|e| ExactMidiEvent {
                            frame: u64::from(e.frame),
                            order: 0,
                            data: e.data.to_vec(),
                        })
                        .collect(),
                };
                let converted = convert_midi(&input, timing, target.sample_rate)?;
                image::Payload::Midi(Arc::new(
                    converted
                        .events
                        .into_iter()
                        .map(|e| {
                            Ok(shoop_app_api::MidiEventState {
                                frame: u32::try_from(e.frame)?,
                                data: e.data.into(),
                            })
                        })
                        .collect::<Result<Vec<_>>>()?,
                ))
            }
        };
        result.payloads.push(payload);
    }
    result
        .validate()
        .map_err(|e| anyhow!("converted driver session is invalid: {e:?}"))?;
    Ok(Arc::new(result))
}
