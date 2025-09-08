use std::{collections::BTreeMap, path::PathBuf};

pub use crate::cxx_qt_shoop::qobj_click_track_generator_bridge::ffi::ClickTrackGenerator;
use crate::cxx_qt_shoop::qobj_click_track_generator_bridge::ffi::*;
use crate::init::GLOBAL_CONFIG;
use backend_bindings::{MidiEvent, MultichannelAudio};
use cxx_qt_lib::QList;

use common::logging::macros::*;
use cxx_qt_lib_shoop::{
    connection_types, invokable::invoke, qvariant_helpers::qvariant_to_qobject_ptr,
};
use sndfile::SndFileIO;
shoop_log_unit!("Frontend.ClickTrackGenerator");

pub fn register_qml_singleton(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_type_clicktrackgenerator(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}

struct ClickTrackTimings {
    pub output_nframes: usize,
    pub beat_start_frames: Vec<usize>,
}

fn generate_click_track_timings(
    bpm: f64,
    n_beats: usize,
    alt_click_delay_percent: f64,
    sample_rate: usize,
) -> ClickTrackTimings {
    let seconds_per_beat = 1.0 / (bpm / 60.0);
    let output_length_seconds = seconds_per_beat * n_beats as f64;
    let beat_start_seconds: Vec<f64> = (0..n_beats).map(|i| i as f64 * seconds_per_beat).collect();
    let output_nframes = (output_length_seconds * sample_rate as f64) as usize;
    let mut beat_start_frames: Vec<usize> = beat_start_seconds
        .iter()
        .map(|s| (s * sample_rate as f64) as usize)
        .collect();

    if alt_click_delay_percent > 0.0 {
        for idx in 0..n_beats {
            if idx % 2 > 0 {
                let frame = beat_start_frames.get_mut(idx).unwrap();
                *frame += (alt_click_delay_percent * sample_rate as f64 * seconds_per_beat / 100.0)
                    as usize;
            }
        }
    }

    ClickTrackTimings {
        output_nframes,
        beat_start_frames,
    }
}

fn _generate_click_track_midi(
    _notes: Vec<u8>,
    _channesl: Vec<u8>,
    _velocities: Vec<u8>,
    _note_length: f32,
    _bpm: f64,
    _n_beats: usize,
    _alt_click_delay_percent: f64,
    _sample_rate: usize,
) -> Vec<MidiEvent> {
    todo!();
}

fn generate_click_track_audio_from_waveforms(
    click_waveforms: &[Vec<f32>],
    bpm: f64,
    n_beats: usize,
    alt_click_delay_percent: f64,
    sample_rate: usize,
) -> Result<Vec<f32>, anyhow::Error> {
    let timings = generate_click_track_timings(bpm, n_beats, alt_click_delay_percent, sample_rate);
    let mut waveform_iter = click_waveforms.iter().cycle();
    let mut output_buffer: Vec<f32> = vec![0.0; timings.output_nframes];
    for start_frame in timings.beat_start_frames.iter() {
        let waveform = waveform_iter
            .next()
            .ok_or(anyhow::anyhow!("No waveforms available"))?;
        for (idx, sample) in waveform.iter().enumerate() {
            if let Some(target_sample) = output_buffer.get_mut(start_frame + idx) {
                *target_sample += sample;
            }
        }
    }

    Ok(output_buffer)
}

fn scan_available_clicks() -> BTreeMap<String, PathBuf> {
    let mut rval: BTreeMap<String, PathBuf> = BTreeMap::new();
    if let Some(cfg) = GLOBAL_CONFIG.get() {
        if let Ok(iter) = glob::glob(&format!("{}/clicks/*.wav", cfg.resource_dir)) {
            for wavefile in iter {
                if let Ok(path) = wavefile {
                    let name = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or("unknown".to_string());
                    rval.insert(name, path);
                }
            }
        }
    }
    rval
}

fn generate_click_track_audio(
    clicks: &[String],
    bpm: f64,
    n_beats: usize,
    alt_click_delay_percent: f64,
    sample_rate: usize,
) -> Result<Vec<f32>, anyhow::Error> {
    let all_clicks = scan_available_clicks();
    let mut click_waveforms: Vec<Vec<f32>> = Vec::new();
    for click in clicks {
        let click_path = all_clicks
            .get(click)
            .ok_or(anyhow::anyhow!("No click '{click}' found"))?;
        let mut audio = sndfile::OpenOptions::ReadOnly(sndfile::ReadOptions::Auto)
            .from_path(click_path)
            .map_err(|e| anyhow::anyhow!("failed to load audio: {e:?}"))?;
        let audio_channels = audio.get_channels();
        let audio_samplerate = audio.get_samplerate();
        let mut data: Vec<f32> = audio
            .read_all_to_vec()
            .map_err(|e| anyhow::anyhow!("failed to read audio: {e:?}"))?;

        // Convert to single-channel
        let mut indices = 0..data.len();
        data.retain(|_| indices.next().unwrap_or(0) % audio_channels == 0);

        // Resample the click if needed.
        if audio_samplerate != sample_rate {
            let mut multichan_audio = MultichannelAudio::new(1, data.len() as u32)?;
            data.iter()
                .enumerate()
                .try_for_each(|(idx, v)| multichan_audio.set(idx as u32, 0, *v))?;
            let new_n_frames =
                (data.len() as f64 * sample_rate as f64 / audio_samplerate as f64) as u32;
            multichan_audio = multichan_audio.resample(new_n_frames)?;
            let mut new_data: Vec<f32> = Vec::new();
            new_data.resize(new_n_frames as usize, 0.0);
            for (idx, v) in new_data.iter_mut().enumerate() {
                *v = multichan_audio.at(idx as u32, 0)?;
            }
            data = new_data;
        }
        click_waveforms.push(data);
    }

    generate_click_track_audio_from_waveforms(
        &click_waveforms,
        bpm,
        n_beats,
        alt_click_delay_percent,
        sample_rate,
    )
}

impl ClickTrackGenerator {
    pub fn get_possible_clicks(self: &ClickTrackGenerator) -> QList_QString {
        let mut rval: QList_QString = QList::default();
        scan_available_clicks()
            .keys()
            .for_each(|k| rval.append(QString::from(k)));
        rval
    }

    pub fn generate_audio(
        self: &ClickTrackGenerator,
        click_names: QList_QString,
        bpm: i32,
        n_beats: i32,
        alt_click_delay_percent: i32,
        sample_rate: usize,
    ) -> QList_f32 {
        match || -> Result<QList_f32, anyhow::Error> {
            let clicks: Vec<String> = click_names.iter().map(|q| q.to_string()).collect();
            let wave = generate_click_track_audio(
                &clicks,
                bpm as f64,
                n_beats as usize,
                alt_click_delay_percent as f64,
                sample_rate as usize,
            )?;

            let mut list: QList_f32 = QList::default();
            list.reserve(wave.len() as isize);
            for v in wave.iter() {
                list.append(*v);
            }

            Ok(list)
        }() {
            Ok(list) => list,
            Err(e) => {
                error!("Failed to preview click track: {e}");
                QList::default()
            }
        }
    }

    pub fn generate_audio_into_channels(
        self: &ClickTrackGenerator,
        click_names: QList_QString,
        bpm: i32,
        n_beats: i32,
        alt_click_delay_percent: i32,
        sample_rate: usize,
        channels: QList_QVariant,
    ) -> i32 {
        match || -> Result<i32, anyhow::Error> {
            let data = self.generate_audio(
                click_names,
                bpm,
                n_beats,
                alt_click_delay_percent,
                sample_rate,
            );

            for channel in channels.iter() {
                let channel = qvariant_to_qobject_ptr(channel)?;
                if channel.is_null() {
                    return Err(anyhow::anyhow!("Null channel"));
                }
                unsafe {
                    invoke::<_, (), _>(
                        &mut *channel,
                        "load_audio_data(QList<float>)",
                        connection_types::DIRECT_CONNECTION,
                        &(data),
                    )?;
                    invoke::<_, (), _>(
                        &mut *channel,
                        "push_start_offset(::std::int32_t)",
                        connection_types::DIRECT_CONNECTION,
                        &(0),
                    )?;
                    invoke::<_, (), _>(
                        &mut *channel,
                        "push_n_preplay_samples(::std::int32_t)",
                        connection_types::DIRECT_CONNECTION,
                        &(0),
                    )?;
                }
            }

            info!("Loaded click track into {} loop channels.", channels.len());

            Ok(data.len() as i32)
        }() {
            Ok(l) => l,
            Err(e) => {
                error!("Failed to generate click track into channel(s): {e}");
                0
            }
        }
    }

    pub fn generate_midi(
        self: &ClickTrackGenerator,
        _notes: QList_i32,
        _channels: QList_i32,
        _velocities: QList_i32,
        _note_length: f32,
        _bpm: i32,
        _n_beats: i32,
        _alt_click_delay_percent: i32,
        _sample_rate: i32,
    ) -> QString {
        todo!();
    }

    pub fn preview_audio(
        self: &ClickTrackGenerator,
        click_names: QList_QString,
        bpm: i32,
        n_beats: i32,
        alt_click_delay_percent: i32,
        sample_rate: i32,
    ) {
        if let Err(e) = || -> Result<(), anyhow::Error> {
            let clicks: Vec<String> = click_names.iter().map(|q| q.to_string()).collect();
            let wave = generate_click_track_audio(
                &clicks,
                bpm as f64,
                n_beats as usize,
                alt_click_delay_percent as f64,
                sample_rate as usize,
            )?;

            std::thread::spawn(move || {
                if let Err(e) = || -> Result<(), anyhow::Error> {
                    let mut stream_handle = rodio::OutputStreamBuilder::open_default_stream()
                        .map_err(|e| anyhow::anyhow!("Failed to open rodio stream: {e}"))?;
                    stream_handle.log_on_drop(false);
                    let sink = rodio::Sink::connect_new(&stream_handle.mixer());
                    let source = rodio::buffer::SamplesBuffer::new(1, sample_rate as u32, wave);

                    // Append the source to the sink
                    sink.append(source);
                    sink.play();
                    sink.sleep_until_end();

                    Ok(())
                }() {
                    error!("Failed to playback click track: {e}");
                }
            });

            Ok(())
        }() {
            error!("Failed to preview click track: {e}");
        }
    }
}
