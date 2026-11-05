use alloc::vec::Vec;

use super::*;

impl Service {
    pub(super) fn advance_media(&mut self) {
        let Some((transfer, loop_id)) = self.media_pending else {
            return;
        };
        let Some((connection, authority)) = self
            .lease
            .as_ref()
            .map(|lease| (lease.connection, lease.authority))
        else {
            return;
        };
        let result = (|| {
            let entry = self.loops.get(&loop_id).ok_or(Refusal::Invalid)?;
            let track = &self.tracks[&entry.track];
            if matches!(
                entry.view.mode,
                LoopMode::Recording | LoopMode::Replacing | LoopMode::RecordingDryIntoWet
            ) {
                return Err(Refusal::NotReady);
            }
            let channels = match track.view.topology {
                TrackTopology::Direct => track.audio_channels,
                TrackTopology::DryWet {
                    dry_audio_channels,
                    wet_audio_channels,
                    ..
                } => dry_audio_channels + wet_audio_channels,
            };
            if entry
                .view
                .length_frames
                .saturating_mul(u64::from(channels))
                .saturating_mul(4)
                > (MAX_MEDIA_BYTES - 4096) as u64
            {
                return Err(Refusal::Overloaded);
            }
            let has_audio = channels > 0;
            let has_midi = track.midi;
            let audio = if has_audio {
                self.read_audio(connection, authority, loop_id)
                    .map_err(|_| Refusal::Engine)?
            } else {
                Some(BackendAudioData::default())
            };
            let midi = if has_midi {
                self.read_midi(connection, authority, loop_id)
                    .map_err(|_| Refusal::Engine)?
            } else {
                Some(BackendMidiData::default())
            };
            let (Some(audio), Some(midi)) = (audio, midi) else {
                return Ok(None);
            };
            let data = LoopMedia {
                content_revision: self.loops[&loop_id].view.content_revision,
                loop_id,
                audio: audio
                    .channels
                    .into_iter()
                    .map(|channel| AudioMediaChannel {
                        samples: channel.samples.to_vec().into(),
                        start_offset: channel.start_offset,
                        capture_alignment_frames: channel.capture_alignment_frames,
                        preplay: channel.preplay,
                    })
                    .collect(),
                midi: midi
                    .channels
                    .into_iter()
                    .map(|channel| MidiMediaChannel {
                        length: channel.length,
                        capture_alignment_frames: channel.capture_alignment_frames,
                        events: channel
                            .events
                            .iter()
                            .cloned()
                            .map(|event| MidiMediaEvent {
                                frame: event.frame,
                                data: event.data.into(),
                            })
                            .collect::<Vec<_>>()
                            .into(),
                        content_revision: channel.content_revision,
                        start_offset: channel.start_offset,
                        preplay: channel.preplay,
                    })
                    .collect(),
            };
            if !data.valid() {
                return Err(Refusal::Overloaded);
            }
            Ok(Some(data))
        })();
        let reply = match result {
            Ok(Some(data)) => ServiceMessage::Media {
                authority,
                transfer,
                data: Arc::new(data),
            },
            Err(reason) => ServiceMessage::MediaRefused {
                authority,
                transfer,
                reason,
            },
            Ok(None) => return,
        };
        self.media_pending = None;
        self.backend.cancel_media_reads();
        self.media_loop = None;
        self.emit(reply);
    }
}
