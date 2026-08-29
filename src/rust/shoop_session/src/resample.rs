use crate::archive::SessionError;
use crate::document::{ExactMidi, LoopAudio, MediaPayload, SessionBundle};
use rubato::{
    audioadapter_buffers::direct::SequentialSliceOfVecs, Async, FixedAsync, Resampler,
    SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use std::collections::BTreeMap;

const MIN_RATIO: f64 = 1.0 / 16.0;
const MAX_RATIO: f64 = 64.0;

pub fn resample_session(
    bundle: &SessionBundle,
    target_sample_rate: u32,
) -> Result<SessionBundle, SessionError> {
    let source = bundle.document.sample_rate;
    validate_rates(source, target_sample_rate)?;
    if source == target_sample_rate {
        return Ok(bundle.clone());
    }
    let mut converted = bundle.clone();
    for payload in converted.media.values_mut() {
        match payload {
            MediaPayload::Audio(audio) => {
                let target =
                    scale_duration(audio.samples.len() as u64, source, target_sample_rate)?;
                audio.samples = resample_mono(&audio.samples, target as usize)?;
            }
            MediaPayload::Midi(midi) => {
                *midi = resample_exact_midi(midi, target_sample_rate)?;
            }
        }
    }
    converted.document.sample_rate = target_sample_rate;
    let mut compensated_media_lengths = BTreeMap::<String, u64>::new();
    for group in &mut converted.document.track_groups {
        for track in &mut group.tracks {
            track.latency.manual_frames =
                scale_signed_nearest(track.latency.manual_frames, source, target_sample_rate)?;
            track.latency.processor_advance_frames = scale_nearest(
                track.latency.processor_advance_frames,
                source,
                target_sample_rate,
            )?;
            for port in &mut track.ports {
                port.ringbuffer_frames =
                    scale_duration(port.ringbuffer_frames, source, target_sample_rate)?;
            }
            if let Some(chain) = &mut track.fx_chain {
                for port in &mut chain.ports {
                    port.ringbuffer_frames =
                        scale_duration(port.ringbuffer_frames, source, target_sample_rate)?;
                }
            }
            for loop_ in &mut track.loops {
                loop_.length_frames =
                    scale_duration(loop_.length_frames, source, target_sample_rate)?;
                for channel in &mut loop_.channels {
                    let compensated = channel.capture_alignment_frames != 0;
                    channel.data_length_frames =
                        scale_duration(channel.data_length_frames, source, target_sample_rate)?;
                    channel.start_offset_frames = scale_signed_nearest(
                        channel.start_offset_frames,
                        source,
                        target_sample_rate,
                    )?;
                    channel.capture_alignment_frames = scale_signed_nearest(
                        channel.capture_alignment_frames,
                        source,
                        target_sample_rate,
                    )?;
                    channel.preplay_frames =
                        scale_duration(channel.preplay_frames, source, target_sample_rate)?;
                    if compensated {
                        let required = i128::from(channel.start_offset_frames)
                            .checked_add(i128::from(channel.capture_alignment_frames))
                            .and_then(|value| value.checked_add(i128::from(loop_.length_frames)))
                            .and_then(|value| u64::try_from(value).ok())
                            .ok_or_else(|| {
                                SessionError::Validation(
                                    "resampled compensated media window is out of range".to_owned(),
                                )
                            })?;
                        if let Some(media_id) = &channel.media_id {
                            compensated_media_lengths
                                .entry(media_id.clone())
                                .and_modify(|length| *length = (*length).max(required))
                                .or_insert(required);
                        }
                    }
                }
            }
        }
    }
    for (media_id, required) in compensated_media_lengths {
        let payload =
            converted
                .media
                .get_mut(&media_id)
                .ok_or_else(|| SessionError::MissingMedia {
                    id: media_id.clone(),
                })?;
        match payload {
            MediaPayload::Audio(audio) if (audio.samples.len() as u64) < required => {
                let required = usize::try_from(required).map_err(|_| {
                    SessionError::Validation(
                        "resampled compensated media length exceeds the platform range".to_owned(),
                    )
                })?;
                let pad = audio.samples.last().copied().unwrap_or(0.0);
                audio.samples.resize(required, pad);
            }
            MediaPayload::Midi(midi) if midi.length_frames < required => {
                midi.length_frames = required;
            }
            _ => {}
        }
    }
    for group in &mut converted.document.track_groups {
        for track in &mut group.tracks {
            for loop_ in &mut track.loops {
                for channel in &mut loop_.channels {
                    if let Some(media_id) = &channel.media_id {
                        channel.data_length_frames = match converted.media.get(media_id) {
                            Some(MediaPayload::Audio(audio)) => audio.samples.len() as u64,
                            Some(MediaPayload::Midi(midi)) => midi.length_frames,
                            None => {
                                return Err(SessionError::MissingMedia {
                                    id: media_id.clone(),
                                });
                            }
                        };
                    }
                }
            }
        }
    }
    for port in &mut converted.document.global_ports {
        port.ringbuffer_frames =
            scale_duration(port.ringbuffer_frames, source, target_sample_rate)?;
    }
    for bus in &mut converted.document.buses {
        for port in &mut bus.ports {
            port.ringbuffer_frames =
                scale_duration(port.ringbuffer_frames, source, target_sample_rate)?;
        }
        if let Some(chain) = &mut bus.fx_chain {
            for port in &mut chain.ports {
                port.ringbuffer_frames =
                    scale_duration(port.ringbuffer_frames, source, target_sample_rate)?;
            }
        }
    }
    crate::archive::validate_bundle(&converted)?;
    Ok(converted)
}

pub fn resample_exact_midi(
    midi: &ExactMidi,
    target_sample_rate: u32,
) -> Result<ExactMidi, SessionError> {
    validate_rates(midi.sample_rate, target_sample_rate)?;
    if midi.sample_rate == target_sample_rate {
        return Ok(midi.clone());
    }
    let length = scale_duration(midi.length_frames, midi.sample_rate, target_sample_rate)?;
    let mut events = Vec::with_capacity(midi.events.len());
    for event in &midi.events {
        let mut converted = event.clone();
        converted.frame = scale_nearest(event.frame, midi.sample_rate, target_sample_rate)?;
        if length > 0 {
            converted.frame = converted.frame.min(length - 1);
        }
        events.push(converted);
    }
    events.sort_by_key(|event| (event.frame, event.order));
    Ok(ExactMidi {
        sample_rate: target_sample_rate,
        length_frames: length,
        start_state: midi.start_state.clone(),
        events,
    })
}

pub fn resample_loop_audio(
    audio: &LoopAudio,
    target_sample_rate: u32,
) -> Result<LoopAudio, SessionError> {
    validate_rates(audio.sample_rate, target_sample_rate)?;
    if audio.sample_rate == target_sample_rate {
        return Ok(audio.clone());
    }
    let mut converted = audio.clone();
    converted.sample_rate = target_sample_rate;
    for channel in &mut converted.channels {
        let target = scale_duration(
            channel.samples.len() as u64,
            audio.sample_rate,
            target_sample_rate,
        )?;
        channel.samples = resample_mono(&channel.samples, target as usize)?;
    }
    Ok(converted)
}

pub fn scale_duration(value: u64, from: u32, to: u32) -> Result<u64, SessionError> {
    validate_rates(from, to)?;
    let numerator = (value as u128)
        .checked_mul(to as u128)
        .ok_or_else(|| SessionError::Validation("sample-domain value overflow".to_owned()))?;
    let result = numerator
        .checked_add(from as u128 - 1)
        .ok_or_else(|| SessionError::Validation("sample-domain value overflow".to_owned()))?
        / from as u128;
    u64::try_from(result)
        .map_err(|_| SessionError::Validation("sample-domain value overflow".to_owned()))
}

pub fn scale_nearest(value: u64, from: u32, to: u32) -> Result<u64, SessionError> {
    validate_rates(from, to)?;
    let numerator = (value as u128)
        .checked_mul(to as u128)
        .and_then(|value| value.checked_add(from as u128 / 2))
        .ok_or_else(|| SessionError::Validation("sample-domain value overflow".to_owned()))?;
    u64::try_from(numerator / from as u128)
        .map_err(|_| SessionError::Validation("sample-domain value overflow".to_owned()))
}

pub fn scale_signed_nearest(value: i64, from: u32, to: u32) -> Result<i64, SessionError> {
    validate_rates(from, to)?;
    let sign = if value < 0 { -1_i128 } else { 1_i128 };
    let magnitude = (value as i128).unsigned_abs();
    let numerator = magnitude
        .checked_mul(to as u128)
        .and_then(|value| value.checked_add(from as u128 / 2))
        .ok_or_else(|| SessionError::Validation("signed sample value overflow".to_owned()))?;
    let magnitude = i128::try_from(numerator / from as u128)
        .map_err(|_| SessionError::Validation("signed sample value overflow".to_owned()))?;
    i64::try_from(sign * magnitude)
        .map_err(|_| SessionError::Validation("signed sample value overflow".to_owned()))
}

fn validate_rates(from: u32, to: u32) -> Result<(), SessionError> {
    if from == 0 || to == 0 {
        Err(SessionError::Validation(
            "sample rates must be non-zero".to_owned(),
        ))
    } else {
        Ok(())
    }
}

fn resample_mono(input: &[f32], target_frames: usize) -> Result<Vec<f32>, SessionError> {
    if target_frames == 0 {
        return Ok(Vec::new());
    }
    if input.is_empty() {
        return Ok(vec![0.0; target_frames]);
    }
    if input.len() == target_frames {
        return Ok(input.to_vec());
    }
    let ratio = (target_frames as f64 / input.len() as f64).clamp(MIN_RATIO, MAX_RATIO);
    let sinc_len = 48;
    let params = SincInterpolationParameters {
        sinc_len,
        f_cutoff: Some(0.95),
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };
    let mut resampler =
        Async::<f32>::new_sinc(ratio, 1.0, &params, input.len(), 1, FixedAsync::Input).map_err(
            |error| SessionError::Validation(format!("resampler construction: {error}")),
        )?;
    let planes = [input.to_vec()];
    let input = SequentialSliceOfVecs::new(&planes, 1, input.len())
        .map_err(|error| SessionError::Validation(format!("resampler input: {error}")))?;
    let mut output = resampler
        .process_all(&input, planes[0].len(), None)
        .map_err(|error| SessionError::Validation(format!("resampling failed: {error}")))?
        .take_data();
    output.truncate(target_frames);
    let pad = output.last().copied().unwrap_or(0.0);
    output.resize(target_frames, pad);
    Ok(output)
}
