use super::{
    convert_midi_with_cancel, invalid, AudioConversion, ConversionPlan, FrameScale, LoopTiming,
    MediaTiming,
};
use crate::{
    validate_bundle, AudioPayload, ConversionTarget, MediaPayload, SessionBundle, SessionDocument,
    SessionError,
};
use std::collections::{BTreeMap, BTreeSet};

struct Asset {
    source: String,
    destination: String,
    timing: MediaTiming,
    periodic: bool,
}

pub struct SessionFormatPlan {
    original: SessionDocument,
    document: SessionDocument,
    assets: Vec<Asset>,
    pub timing: ConversionPlan,
    pub media_bytes: u64,
    limit: u64,
}

impl ConversionPlan {
    pub fn for_session(
        bundle: &SessionBundle,
        target: ConversionTarget,
    ) -> Result<Self, SessionError> {
        let mut resolved = bundle.document.clone();
        crate::materialize_composite_timing(&mut resolved)?;
        let document = &resolved;
        let sources = document
            .track_groups
            .iter()
            .flat_map(|group| &group.tracks)
            .filter(|track| track.is_sync)
            .flat_map(|track| &track.loops)
            .filter(|loop_| loop_.is_sync)
            .map(|loop_| loop_.id)
            .collect::<Vec<_>>();
        if sources.len() > 1 {
            return Err(invalid("session has multiple production sync sources"));
        }
        let source = document
            .global
            .sync
            .then(|| sources.first().copied())
            .flatten();
        let mut loops = Vec::new();
        for track in document.track_groups.iter().flat_map(|group| &group.tracks) {
            for loop_ in &track.loops {
                loops.push(LoopTiming {
                    id: loop_.id,
                    sync_source: if loop_.composite.is_some() {
                        None
                    } else {
                        loop_
                            .execution
                            .as_ref()
                            .map_or(if track.is_sync { None } else { source }, |execution| {
                                execution.sync_source
                            })
                    },
                    frames: loop_.length_frames,
                    sample_rate: document.sample_rate,
                });
            }
        }
        let mut plan = Self::new(target, &loops)?;
        plan.requires_confirmation |= document
            .track_groups
            .iter()
            .flat_map(|group| &group.tracks)
            .flat_map(|track| &track.loops)
            .zip(
                bundle
                    .document
                    .track_groups
                    .iter()
                    .flat_map(|group| &group.tracks)
                    .flat_map(|track| &track.loops),
            )
            .any(|(resolved, original)| resolved.length_frames != original.length_frames);
        for loop_ in document
            .track_groups
            .iter()
            .flat_map(|g| &g.tracks)
            .flat_map(|t| &t.loops)
        {
            if let Some(timing) = loop_.composite.as_ref().and_then(|c| c.timing.as_ref()) {
                let converted = timing.converted(target)?;
                plan.requires_confirmation |= timing.instances != converted.instances
                    || timing.spans.iter().zip(&converted.spans).any(|(old, new)| {
                        old.start * timing.unit_frames != new.start * converted.unit_frames
                            || old.end * timing.unit_frames != new.end * converted.unit_frames
                    })
                    || timing.spans.len() != converted.spans.len();
            }
        }
        plan.requires_confirmation |= document.sample_rate != target.sample_rate;
        Ok(plan)
    }
}

impl SessionFormatPlan {
    pub fn new(
        bundle: &SessionBundle,
        target: ConversionTarget,
        limit: u64,
    ) -> Result<Self, SessionError> {
        validate_bundle(bundle)?;
        let timing = ConversionPlan::for_session(bundle, target)?;
        let mut document = bundle.document.clone();
        crate::materialize_composite_timing(&mut document)?;
        let mut assets = Vec::new();
        let mut mappings = BTreeMap::<(String, FrameScale, i128, bool), String>::new();
        let mut assigned = BTreeSet::new();
        let mut reserved = bundle.media.keys().cloned().collect::<BTreeSet<_>>();
        let mut media_bytes = 0_u64;
        for track in document
            .track_groups
            .iter_mut()
            .flat_map(|group| &mut group.tracks)
        {
            for loop_ in &mut track.loops {
                let conversion = timing
                    .get(loop_.id)
                    .ok_or_else(|| invalid("missing loop conversion"))?;
                let old_length = loop_.length_frames;
                loop_.length_frames = conversion.frames;
                if let Some(composite) = &mut loop_.composite {
                    let converted = composite
                        .timing
                        .as_ref()
                        .ok_or_else(|| invalid("missing composite timing"))?
                        .converted(target)?;
                    if converted.unit_frames.checked_mul(converted.iterations)
                        != Some(conversion.frames)
                    {
                        return Err(invalid(
                            "composite conversion disagrees with session duration",
                        ));
                    }
                    composite.timing = Some(converted);
                }
                for channel in &mut loop_.channels {
                    let origin = channel
                        .start_offset_frames
                        .checked_add(channel.capture_alignment_frames)
                        .ok_or_else(|| invalid("combined media origin overflow"))?;
                    let map = MediaTiming::new(conversion.scale, origin)?;
                    let alignment = i64::try_from(
                        conversion
                            .scale
                            .signed_floor(i128::from(channel.capture_alignment_frames))?,
                    )
                    .map_err(|_| invalid("capture alignment overflow"))?;
                    channel.start_offset_frames = map
                        .target_origin
                        .checked_sub(alignment)
                        .ok_or_else(|| invalid("channel offset overflow"))?;
                    channel.capture_alignment_frames = alignment;
                    channel.preplay_frames = conversion.scale.ceil(channel.preplay_frames)?;
                    channel.data_length_frames = map.length(channel.data_length_frames)?;
                    let Some(source) = channel.media_id.clone() else {
                        continue;
                    };
                    let periodic = !map.scale.is_identity()
                        && matches!(bundle.media[&source], MediaPayload::Audio(_))
                        && origin == 0
                        && old_length > 0
                        && payload_frames(&bundle.media[&source]) == old_length;
                    let phase = i128::from(map.target_origin)
                        .checked_mul(
                            i128::try_from(map.scale.denominator())
                                .map_err(|_| invalid("phase overflow"))?,
                        )
                        .and_then(|scaled| {
                            i128::from(map.origin)
                                .checked_mul(i128::try_from(map.scale.numerator()).ok()?)
                                .and_then(|original| scaled.checked_sub(original))
                        })
                        .ok_or_else(|| invalid("phase overflow"))?;
                    let key = (source.clone(), map.scale, phase, periodic);
                    let destination = if let Some(destination) = mappings.get(&key) {
                        destination.clone()
                    } else {
                        let destination = if assigned.insert(source.clone()) {
                            source.clone()
                        } else {
                            let mut suffix = assets.len();
                            loop {
                                let id = format!("converted_{suffix}");
                                if reserved.insert(id.clone()) {
                                    break id;
                                }
                                suffix += 1;
                            }
                        };
                        media_bytes = media_bytes
                            .checked_add(payload_bytes(&bundle.media[&source], map)?)
                            .ok_or_else(|| invalid("converted media budget overflow"))?;
                        if media_bytes > limit {
                            return Err(SessionError::ResourceLimit(
                                "converted session media exceeds budget".into(),
                            ));
                        }
                        assets.push(Asset {
                            source: source.clone(),
                            destination: destination.clone(),
                            timing: map,
                            periodic,
                        });
                        mappings.insert(key, destination.clone());
                        destination
                    };
                    channel.media_id = Some(destination);
                }
            }
        }
        let rate_scale = FrameScale::new(
            u128::from(target.sample_rate),
            u128::from(bundle.document.sample_rate),
        )?;
        for (source, payload) in &bundle.media {
            if assigned.contains(source) {
                continue;
            }
            let map = MediaTiming::new(rate_scale, 0)?;
            media_bytes = media_bytes
                .checked_add(payload_bytes(payload, map)?)
                .ok_or_else(|| invalid("converted media budget overflow"))?;
            if media_bytes > limit {
                return Err(SessionError::ResourceLimit(
                    "converted session media exceeds budget".into(),
                ));
            }
            assets.push(Asset {
                source: source.clone(),
                destination: source.clone(),
                timing: map,
                periodic: false,
            });
        }
        for track in document
            .track_groups
            .iter_mut()
            .flat_map(|group| &mut group.tracks)
        {
            track.latency.manual_frames = scale_physical(track.latency.manual_frames, rate_scale)?;
            track.latency.processor_manual_frames =
                scale_physical(track.latency.processor_manual_frames, rate_scale)?;
            for port in &mut track.ports {
                port.ringbuffer_frames = rate_scale.ceil(port.ringbuffer_frames)?;
            }
            if let Some(chain) = &mut track.fx_chain {
                for port in &mut chain.ports {
                    port.ringbuffer_frames = rate_scale.ceil(port.ringbuffer_frames)?;
                }
            }
        }
        for port in &mut document.global_ports {
            port.ringbuffer_frames = rate_scale.ceil(port.ringbuffer_frames)?;
        }
        for bus in &mut document.buses {
            for port in &mut bus.ports {
                port.ringbuffer_frames = rate_scale.ceil(port.ringbuffer_frames)?;
            }
            if let Some(chain) = &mut bus.fx_chain {
                for port in &mut chain.ports {
                    port.ringbuffer_frames = rate_scale.ceil(port.ringbuffer_frames)?;
                }
            }
        }
        document.sample_rate = target.sample_rate;
        for (converted, original) in document
            .track_groups
            .iter_mut()
            .flat_map(|g| &mut g.tracks)
            .flat_map(|t| &mut t.loops)
            .zip(
                bundle
                    .document
                    .track_groups
                    .iter()
                    .flat_map(|g| &g.tracks)
                    .flat_map(|t| &t.loops),
            )
        {
            if original.execution.is_none() {
                converted.execution = None;
            }
        }
        Ok(Self {
            original: bundle.document.clone(),
            document,
            assets,
            timing,
            media_bytes,
            limit,
        })
    }

    pub fn convert(
        self,
        bundle: &SessionBundle,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<SessionBundle, SessionError> {
        if bundle.document != self.original {
            return Err(invalid("session changed after conversion planning"));
        }
        validate_bundle(bundle)?;
        let mut media = BTreeMap::new();
        for asset in self.assets {
            if cancelled() {
                return Err(invalid("session conversion cancelled"));
            }
            let input =
                bundle
                    .media
                    .get(&asset.source)
                    .ok_or_else(|| SessionError::MissingMedia {
                        id: asset.source.clone(),
                    })?;
            let payload = match input {
                MediaPayload::Audio(audio) => {
                    let mut conversion = AudioConversion::new(
                        &audio.samples,
                        asset.timing,
                        asset.periodic,
                        self.limit / 4,
                    )?;
                    while !conversion.step(4096)? {
                        if cancelled() {
                            return Err(invalid("session conversion cancelled"));
                        }
                    }
                    MediaPayload::Audio(AudioPayload {
                        samples: conversion.finish()?,
                    })
                }
                MediaPayload::Midi(midi) => MediaPayload::Midi(convert_midi_with_cancel(
                    midi,
                    asset.timing,
                    self.timing.target.sample_rate,
                    &mut cancelled,
                )?),
            };
            media.insert(asset.destination, payload);
        }
        let converted = SessionBundle {
            document: self.document,
            media,
            scripts: bundle.scripts.clone(),
        };
        validate_bundle(&converted)?;
        Ok(converted)
    }
}

fn scale_physical(value: i64, scale: FrameScale) -> Result<i64, SessionError> {
    let magnitude = scale.multiply(FrameScale::new(u128::from(value.unsigned_abs()), 1)?)?;
    let rounded = magnitude.numerator() / magnitude.denominator()
        + u128::from(
            magnitude.numerator() % magnitude.denominator()
                >= magnitude.denominator() / 2 + magnitude.denominator() % 2,
        );
    let rounded = u64::try_from(rounded).map_err(|_| invalid("physical frame field overflow"))?;
    let signed = if value < 0 {
        -i128::from(rounded)
    } else {
        i128::from(rounded)
    };
    i64::try_from(signed).map_err(|_| invalid("physical frame field overflow"))
}

fn payload_frames(payload: &MediaPayload) -> u64 {
    match payload {
        MediaPayload::Audio(audio) => audio.samples.len() as u64,
        MediaPayload::Midi(midi) => midi.length_frames,
    }
}

fn payload_bytes(payload: &MediaPayload, timing: MediaTiming) -> Result<u64, SessionError> {
    match payload {
        MediaPayload::Audio(audio) => timing
            .length(audio.samples.len() as u64)?
            .checked_mul(4)
            .ok_or_else(|| invalid("audio size overflow")),
        MediaPayload::Midi(midi) => midi
            .events
            .iter()
            .try_fold(0_u64, |bytes, event| {
                bytes.checked_add(event.data.len() as u64 + 12)
            })
            .and_then(|bytes| {
                midi.start_state.iter().try_fold(bytes, |bytes, packet| {
                    bytes.checked_add(packet.len() as u64 + 4)
                })
            })
            .ok_or_else(|| invalid("MIDI size overflow")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{decode_session, encode_session};
    use shoop_wasm_test_support::shoop_test;

    fn target(quantum_frames: u32) -> ConversionTarget {
        ConversionTarget {
            sample_rate: 48_000,
            quantum_frames,
        }
    }

    #[shoop_test]
    fn session_conversion_is_identity_when_timings_fit_and_reload_is_idempotent() {
        let bundle = crate::tests::direct_bundle(2);
        let identity = SessionFormatPlan::new(&bundle, target(1), 1_000_000).unwrap();
        assert!(!identity.timing.requires_confirmation);
        assert_eq!(identity.convert(&bundle, || false).unwrap(), bundle);
        let plan = SessionFormatPlan::new(&bundle, target(32), 1_000_000).unwrap();
        assert!(plan.timing.requires_confirmation);
        let converted = plan.convert(&bundle, || false).unwrap();
        assert_eq!(
            converted.document.track_groups[0].tracks[0].loops[0].length_frames,
            288
        );
        assert_eq!(
            converted.document.track_groups[0].tracks[0].ports,
            bundle.document.track_groups[0].tracks[0].ports
        );
        let saved = encode_session(&converted, "retiming-test").unwrap();
        let loaded = decode_session(&saved).unwrap();
        let plan = SessionFormatPlan::new(&loaded, target(32), 1_000_000).unwrap();
        assert!(!plan.timing.requires_confirmation);
        assert_eq!(plan.convert(&loaded, || false).unwrap(), converted);
        assert_eq!(
            bundle.document.track_groups[0].tracks[0].loops[0].length_frames,
            301
        );
    }

    #[shoop_test]
    fn shared_assets_split_by_mapping_and_identical_maps_deduplicate() {
        let mut bundle = crate::tests::direct_bundle(1);
        let track = &mut bundle.document.track_groups[0].tracks[0];
        let mut second = track.loops[0].clone();
        second.id = 11;
        second.length_frames = 640;
        for channel in &mut second.channels {
            channel.id += 10_000;
        }
        let mut third = second.clone();
        third.id = 12;
        for channel in &mut third.channels {
            channel.id += 10_000;
        }
        track.loops.extend([second, third]);
        let converted = SessionFormatPlan::new(&bundle, target(32), 1_000_000)
            .unwrap()
            .convert(&bundle, || false)
            .unwrap();
        let loops = &converted.document.track_groups[0].tracks[0].loops;
        assert_ne!(loops[0].channels[0].media_id, loops[1].channels[0].media_id);
        assert_eq!(loops[1].channels[0].media_id, loops[2].channels[0].media_id);
        assert_eq!(converted.media.len(), bundle.media.len() * 2);
        for channel in &loops[1].channels {
            let original = bundle
                .media
                .get(channel.media_id.as_ref().unwrap())
                .or_else(|| {
                    let index = loops[1]
                        .channels
                        .iter()
                        .position(|candidate| candidate.id == channel.id)
                        .unwrap();
                    bundle.media.get(
                        bundle.document.track_groups[0].tracks[0].loops[0].channels[index]
                            .media_id
                            .as_ref()
                            .unwrap(),
                    )
                })
                .unwrap();
            assert_eq!(
                &converted.media[channel.media_id.as_ref().unwrap()],
                original
            );
        }
    }

    #[shoop_test]
    fn combined_rate_conversion_maps_compensated_windows_and_physical_latency() {
        let mut bundle = crate::tests::direct_bundle(1);
        let track = &mut bundle.document.track_groups[0].tracks[0];
        track.latency.manual_frames = 11;
        track.latency.processor_manual_frames = 7;
        let loop_ = &mut track.loops[0];
        loop_.length_frames = 2;
        for channel in &mut loop_.channels {
            channel.start_offset_frames = -1;
            channel.capture_alignment_frames = 2;
        }
        let destination = ConversionTarget {
            sample_rate: 32_000,
            quantum_frames: 32,
        };
        let plan = SessionFormatPlan::new(&bundle, destination, 1_000_000).unwrap();
        let converted = plan.convert(&bundle, || false).unwrap();
        let track = &converted.document.track_groups[0].tracks[0];
        assert_eq!(track.latency.manual_frames, 7);
        assert_eq!(track.latency.processor_manual_frames, 5);
        assert_eq!(track.loops[0].length_frames, 32);
        for channel in &track.loops[0].channels {
            let origin = channel.start_offset_frames + channel.capture_alignment_frames;
            assert_eq!(origin, 16);
            assert!(channel.data_length_frames >= origin as u64 + 32);
        }
        validate_bundle(&converted).unwrap();
    }

    #[shoop_test]
    fn planning_budgets_and_cancelled_or_stale_work_preserve_original() {
        let bundle = crate::tests::direct_bundle(1);
        assert!(SessionFormatPlan::new(&bundle, target(32), 1).is_err());
        let plan = SessionFormatPlan::new(&bundle, target(32), 1_000_000).unwrap();
        assert!(plan
            .convert(&bundle, || true)
            .unwrap_err()
            .to_string()
            .contains("cancelled"));
        let plan = SessionFormatPlan::new(&bundle, target(32), 1_000_000).unwrap();
        let mut stale = bundle.clone();
        stale.document.global.solo = false;
        assert!(plan
            .convert(&stale, || false)
            .unwrap_err()
            .to_string()
            .contains("changed"));
        validate_bundle(&bundle).unwrap();
    }

    #[shoop_test]
    fn session_sync_source_retimes_dependents_and_empty_source_is_independent() {
        let mut bundle = crate::tests::direct_bundle(1);
        bundle.document.track_groups[0].tracks[0].loops[0].length_frames = 192_064;
        let mut sync = bundle.document.track_groups[0].tracks[0].clone();
        sync.id = 2;
        sync.is_sync = true;
        sync.fx_chain = None;
        sync.loops[0].id = 20;
        sync.loops[0].is_sync = true;
        sync.loops[0].length_frames = 48_016;
        for port in &mut sync.ports {
            port.id += 5000;
            for connection in &mut port.internal_connections {
                *connection += 5000;
            }
        }
        for channel in &mut sync.loops[0].channels {
            channel.id += 5000;
            channel.recording_fx_state_id = None;
            for port in &mut channel.connected_port_ids {
                *port += 5000;
            }
        }
        bundle
            .document
            .track_groups
            .push(crate::TrackGroupDocument {
                name: "sync".into(),
                tracks: vec![sync],
            });
        let plan = SessionFormatPlan::new(&bundle, target(32), 1_000_000).unwrap();
        assert_eq!(plan.timing.get(20).unwrap().frames, 48_032);
        assert_eq!(plan.timing.get(10).unwrap().frames, 192_128);
        let converted = plan.convert(&bundle, || false).unwrap();
        validate_bundle(&converted).unwrap();
        bundle.document.track_groups[1].tracks[0].loops[0].length_frames = 0;
        let plan = SessionFormatPlan::new(&bundle, target(32), 1_000_000).unwrap();
        assert_eq!(plan.timing.get(20).unwrap().frames, 0);
        assert_eq!(plan.timing.get(10).unwrap().frames, 192_064);
    }

    #[shoop_test]
    fn midi_collision_keeps_source_order_and_remains_a_valid_archive() {
        let mut bundle = crate::tests::direct_bundle(1);
        bundle.document.track_groups[0].tracks[0].loops[0]
            .channels
            .iter_mut()
            .find(|channel| channel.data_type == crate::DataTypeDocument::Midi)
            .unwrap()
            .start_offset_frames = 0;
        let MediaPayload::Midi(midi) = bundle.media.get_mut("midi_main").unwrap() else {
            panic!("MIDI fixture");
        };
        midi.events[0].frame = 0;
        midi.events[0].order = 9;
        midi.events[1].frame = 1;
        midi.events[1].order = 2;
        let bytes = midi
            .events
            .iter()
            .map(|event| event.data.clone())
            .collect::<Vec<_>>();
        let converted = SessionFormatPlan::new(&bundle, target(32), 1_000_000)
            .unwrap()
            .convert(&bundle, || false)
            .unwrap();
        let MediaPayload::Midi(midi) = &converted.media["midi_main"] else {
            panic!("MIDI fixture");
        };
        assert_eq!(
            midi.events
                .iter()
                .map(|event| event.frame)
                .collect::<Vec<_>>(),
            [0, 0]
        );
        assert_eq!(
            midi.events
                .iter()
                .map(|event| event.order)
                .collect::<Vec<_>>(),
            [0, 1]
        );
        assert_eq!(
            midi.events
                .iter()
                .map(|event| event.data.clone())
                .collect::<Vec<_>>(),
            bytes
        );
        validate_bundle(&converted).unwrap();
        let saved = encode_session(&converted, "collision-test").unwrap();
        assert_eq!(decode_session(&saved).unwrap(), converted);
    }
}
