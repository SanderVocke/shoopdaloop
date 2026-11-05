use super::*;
use alloc::sync::Arc;
use shoop_app_api::{self as api, IoTaskKind, MidiEventState};
use shoop_app_protocol::persistence_image as image;
use shoop_persistence_codec::codec::{Decoded, Value, Work};
use shoop_session::{ConversionTarget, ExactMidi, ExactMidiEvent};

struct Pinned {
    audio: Vec<u8>,
    midi: Vec<u8>,
    initial: MidiState,
    packets: Vec<Vec<u8>>,
    frames: u64,
}
impl Pinned {
    fn capture(h: &mut Fixture, (id, export): (media::ExportId, media::Export)) -> Self {
        let mut channels = [*MidiState::default().channel(0).unwrap(); 16];
        let mut reset = false;
        for ch in 0..16 {
            let Reply::MidiInitial {
                state,
                system_reset_seen,
                ..
            } = h.op(Operation::MidiInitial {
                export: id,
                channel: ch,
            })
            else {
                panic!("initial projection")
            };
            channels[ch as usize] = state;
            reset = system_reset_seen;
        }
        let initial = MidiState::from_channels(channels, reset);
        let mut packets = Vec::new();
        let restored = shoop_loops::midi_restore::restore(&initial, |p| {
            packets.push(p.to_vec());
            Ok::<(), Error>(())
        });
        assert_eq!(restored.first_error, None);
        assert!(!restored.parameter_history_untracked);
        Self {
            audio: h.read(export.view.audio.unwrap().snapshot),
            midi: h.read(export.view.midi.unwrap().snapshot),
            initial,
            packets,
            frames: export.view.description.frames,
        }
    }
    fn exact(&self) -> ExactMidi {
        ExactMidi {
            sample_rate: 1000,
            length_frames: self.frames,
            start_state: self.packets.clone(),
            events: midi_events(&self.midi)
                .into_iter()
                .enumerate()
                .map(|(order, (frame, data))| ExactMidiEvent {
                    frame,
                    order: order as u32,
                    data,
                })
                .collect(),
        }
    }
    fn image(&self) -> Arc<image::Image> {
        let channel = |object| image::Channel {
            mode: image::ChannelMode::Direct,
            object: Some(object),
            frames: self.frames,
            gain: 1.,
            start_offset: 0,
            capture_alignment: 0,
            preplay: 0,
            recording_started_at: None,
        };
        let content = image::Loop {
            recorded_processor_state: None,
            sync_source: None,
            repeat_sync: false,
            composite: None,
            id: api::LoopId::from_raw(1),
            name: "fixed shared take".into(),
            sync: false,
            frames: self.frames,
            gain: 1.,
            balance: 0.,
            audio: vec![channel(1), channel(2)],
            midi: vec![image::MidiChannel {
                channel: channel(3),
                initial: self.packets.clone(),
            }],
        };
        let audio: Vec<f32> = self
            .audio
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        Arc::new(image::Image {
            manifest: image::Manifest::Session(image::Session {
                sample_rate: 1000,
                global: api::GlobalControlState::default(),
                tracks: vec![image::Track {
                    topology: shoop_app_api::TrackTopology::Direct,
                    default_playback_mode: Default::default(),
                    processor_state: None,
                    builtin_fx_midi_cc_assignments: Vec::new(),
                    oxisynth_midi_cc_assignments: Vec::new(),
                    id: api::TrackId::from_raw(1),
                    name: "input".into(),
                    port_name_base: "input".into(),
                    sync: false,
                    audio_channels: 2,
                    midi: true,
                    ports: vec![],
                    loops: vec![content],
                    controls: image::Controls {
                        input_gain_db: 0.,
                        input_balance: 0.,
                        monitoring: false,
                        output_gain_db: 0.,
                        output_balance: 0.,
                        muted: false,
                    },
                }],
                buses: vec![],
                bus_display_order: vec![],
                preview_bus: None,
                global_ports: vec![],
                routes: vec![],
                selected: vec![],
                target: None,
            }),
            payloads: vec![
                image::Payload::Audio(Arc::new(audio.iter().step_by(2).copied().collect())),
                image::Payload::Audio(Arc::new(audio.iter().skip(1).step_by(2).copied().collect())),
                image::Payload::Midi(Arc::new(
                    midi_events(&self.midi)
                        .into_iter()
                        .map(|(frame, data)| MidiEventState {
                            frame: frame as u32,
                            data: data.into(),
                        })
                        .collect(),
                )),
            ],
        })
    }
    fn round_trip(&self) {
        let original = self.image();
        let Value::File { bytes, .. } = (Work::Encode {
            image: original.clone(),
            kind: IoTaskKind::SaveSession,
            wav: false,
            standard: false,
            channels: vec![],
        })
        .run()
        .unwrap() else {
            panic!("save")
        };
        let Value::Decoded(decoded) = (Work::Decode {
            kind: IoTaskKind::LoadSession,
            bytes,
            rate: 1000,
        })
        .run()
        .unwrap() else {
            panic!("decode")
        };
        let Value::Decoded(identity) = (Work::Convert {
            decoded,
            target: ConversionTarget {
                sample_rate: 1000,
                quantum_frames: 32,
            },
            update_length: true,
        })
        .run()
        .unwrap() else {
            panic!("identity")
        };
        let Value::Image(reloaded) = (Work::Prepare {
            decoded: identity,
            mapping: vec![],
            update_length: false,
        })
        .run()
        .unwrap() else {
            panic!("reload")
        };
        assert_eq!(reloaded.payloads, original.payloads);
        let image::Manifest::Session(session) = &reloaded.manifest else {
            panic!("session")
        };
        assert_eq!(session.tracks[0].loops[0].midi[0].initial, self.packets);
        assert_eq!(session.tracks[0].loops[0].frames, self.frames);

        let image::Manifest::Session(session) = &original.manifest else {
            unreachable!()
        };
        let loop_image = Arc::new(image::Image {
            manifest: image::Manifest::Loop {
                sample_rate: 1000,
                content: session.tracks[0].loops[0].clone(),
            },
            payloads: original.payloads.clone(),
        });
        for (kind, wav, standard) in [
            (IoTaskKind::ExportLoopAudio, false, false),
            (IoTaskKind::ExportLoopAudio, true, false),
            (IoTaskKind::ExportLoopMidi, false, false),
            (IoTaskKind::ExportLoopMidi, false, true),
        ] {
            let Value::File { bytes, warning, .. } = (Work::Encode {
                image: loop_image.clone(),
                kind,
                wav,
                standard,
                channels: if kind == IoTaskKind::ExportLoopAudio {
                    vec![0, 1]
                } else {
                    vec![]
                },
            })
            .run()
            .unwrap() else {
                panic!("file")
            };
            if standard {
                assert!(warning.contains("quantizes timing"));
                continue;
            }
            let kind = if kind == IoTaskKind::ExportLoopAudio {
                IoTaskKind::ImportLoopAudio
            } else {
                IoTaskKind::ImportLoopMidi
            };
            let Value::Decoded(decoded) = (Work::Decode {
                kind,
                bytes,
                rate: 1000,
            })
            .run()
            .unwrap() else {
                panic!("file decode")
            };
            match decoded {
                Decoded::Midi(m) => assert_eq!(m, self.exact()),
                Decoded::Audio(a) => {
                    assert_eq!(a.channels.len(), 2);
                    for (ch, data) in a.channels.iter().enumerate() {
                        let image::Payload::Audio(samples) = &original.payloads[ch] else {
                            unreachable!()
                        };
                        assert_eq!(&data.samples, samples.as_ref());
                    }
                }
                _ => panic!("standalone media"),
            }
        }
        let Value::Decoded(Decoded::Midi(converted)) = (Work::Convert {
            decoded: Decoded::Midi(self.exact()),
            target: ConversionTarget {
                sample_rate: 2000,
                quantum_frames: 32,
            },
            update_length: true,
        })
        .run()
        .unwrap() else {
            panic!("rate conversion")
        };
        assert_eq!(converted.start_state, self.packets);
        assert_eq!(converted.length_frames, self.frames * 2);
        for (new, old) in converted.events.iter().zip(self.exact().events) {
            assert_eq!(new.frame, old.frame * 2);
            assert_eq!(new.data, old.data);
        }
        let root = api::LoopId::from_raw(1);
        let child = api::LoopId::from_raw(2);
        let mut dependent = session.tracks[0].loops[0].clone();
        dependent.id = child;
        dependent.frames *= 2;
        for channel in &mut dependent.audio {
            channel.object = channel.object.map(|object| object + 3);
        }
        for channel in &mut dependent.midi {
            channel.channel.object = channel.channel.object.map(|object| object + 3);
        }
        let mut shorter = self.exact();
        shorter.length_frames /= 2;
        shorter
            .events
            .retain(|event| event.frame < shorter.length_frames);
        let base = Arc::new(image::Image {
            manifest: image::Manifest::FamilyUpdate {
                format: ConversionTarget {
                    sample_rate: 1000,
                    quantum_frames: self.frames as u32,
                },
                root,
                members: vec![
                    image::FamilyMember {
                        loop_id: root,
                        sync_source: None,
                        expected_revision: 1,
                        expected_frames: self.frames,
                        fence_content: true,
                        restart: true,
                        content: Some(session.tracks[0].loops[0].clone()),
                    },
                    image::FamilyMember {
                        loop_id: child,
                        sync_source: Some(root),
                        expected_revision: 1,
                        expected_frames: self.frames * 2,
                        fence_content: true,
                        restart: false,
                        content: Some(dependent),
                    },
                ],
            },
            payloads: original
                .payloads
                .iter()
                .cloned()
                .chain(original.payloads.iter().cloned())
                .collect(),
        });
        let Value::Image(family) = (Work::ConvertFamily {
            decoded: Decoded::Midi(shorter.clone()),
            base,
        })
        .run()
        .unwrap() else {
            panic!("shared family conversion")
        };
        let image::Manifest::FamilyUpdate { members, .. } = &family.manifest else {
            panic!("family manifest")
        };
        assert_eq!(members[0].content.as_ref().unwrap().frames, self.frames);
        assert_eq!(members[1].content.as_ref().unwrap().frames, self.frames * 4);
        for member in members {
            let content = member.content.as_ref().unwrap();
            assert_eq!(content.midi[0].initial, self.packets);
            for channel in &content.audio {
                let image::Payload::Audio(samples) =
                    &family.payloads[channel.object.unwrap() as usize - 1]
                else {
                    panic!("family audio")
                };
                assert_eq!(samples.len(), self.frames as usize * 2);
                assert!(samples.iter().all(|sample| sample.is_finite()));
            }
            let image::Payload::Midi(events) =
                &family.payloads[content.midi[0].channel.object.unwrap() as usize - 1]
            else {
                panic!("family MIDI")
            };
            let expected: Vec<_> = if member.loop_id == root {
                shorter
                    .events
                    .iter()
                    .map(|e| (e.frame, e.data.clone()))
                    .collect()
            } else {
                midi_events(&self.midi)
            };
            assert_eq!(events.len(), expected.len());
            for (new, (old_frame, packet)) in events.iter().zip(expected) {
                assert_eq!(u64::from(new.frame), old_frame * 2);
                assert_eq!(new.data.as_ref(), packet);
            }
        }
    }
    fn loaded(&self, h: &mut Fixture, stores: SharedTake) -> SharedTake {
        let mut upload = |format, data: &[u8]| {
            let resource::Reply::Created(resource) = h.resource(ResourceRequest::Create) else {
                panic!("create")
            };
            let description = resource::Description {
                format,
                frames: self.frames,
                bytes: data.len(),
            };
            let request = if format == resource::Format::Midi {
                ResourceRequest::BeginIndexedMidiUpload {
                    resource,
                    expected_revision: rt_runtime::Revision(0),
                    description,
                    initial: Box::new(self.initial),
                }
            } else {
                ResourceRequest::BeginUpload {
                    resource,
                    expected_revision: rt_runtime::Revision(0),
                    description,
                }
            };
            let resource::Reply::Upload(upload) = h.resource(request) else {
                panic!("upload")
            };
            for (page, bytes) in data.chunks(256).enumerate() {
                h.resource(ResourceRequest::Write {
                    upload,
                    offset: page * 256,
                    bytes: bytes.into(),
                });
            }
            h.resource(ResourceRequest::Finish { upload });
            h.resource(ResourceRequest::Commit { upload });
            resource
        };
        SharedTake {
            indexed_midi_store: None,
            frames: self.frames,
            audio: Some(upload(resource::Format::Audio { channels: 2 }, &self.audio)),
            midi: Some(upload(resource::Format::Midi, &self.midi)),
            ..stores
        }
    }
    fn content(&self, h: &mut Fixture, id: u64) {
        for (id, midi, data) in [(id, false, &self.audio), (id + 1, true, &self.midi)] {
            h.request(protocol::Request::ContentBegin(
                protocol::ContentDescription {
                    id,
                    midi,
                    frames: self.frames,
                    bytes: data.len() as u64,
                },
            ));
            if midi {
                for packet in &self.packets {
                    h.request(protocol::Request::ContentInitial {
                        id,
                        message: protocol::MidiMessage::new(packet).unwrap(),
                    });
                }
            }
            for (chunk, bytes) in data.chunks(256).enumerate() {
                h.request(protocol::Request::ContentWrite {
                    id,
                    offset: (chunk * 256) as u64,
                    data: protocol::ContentChunk::new(bytes).unwrap(),
                });
            }
            h.request(protocol::Request::ContentFinish { id });
        }
    }
}

#[shoop_test]
fn loaded_media_longer_than_live_history_uses_bounded_candidate_import_and_keeps_its_base() {
    let mut h = Fixture::new(32, 31);
    let (take, _, _) = h.grabs();
    h.adopt(0, take);
    let export = h.export_owned(0);
    let mut pinned = Pinned::capture(&mut h, export);
    let events = midi_events(&pinned.midi);
    pinned.audio = pinned.audio.repeat(4);
    pinned.midi.clear();
    for period in 0..4 {
        for (frame, packet) in &events {
            let mut record = vec![0; 12 + packet.len()];
            resource::encode_midi_record(period * pinned.frames + frame, packet, &mut record)
                .unwrap();
            pinned.midi.extend_from_slice(&record);
        }
    }
    pinned.frames *= 4;
    assert!(pinned.frames > 8 * h.q as u64);
    let loaded = pinned.loaded(&mut h, take);
    h.adopt(0, loaded);
    let base = h.export(0);
    let Reply::Candidate {
        id,
        status: CandidateStatus::Building,
        ..
    } = h.op(Operation::CandidateBegin {
        target: target(0),
        take: loaded,
    })
    else {
        panic!("loaded candidate")
    };
    for _ in 0..128 {
        if matches!(
            h.op(Operation::CandidateStatus { id }),
            Reply::Candidate {
                status: CandidateStatus::Ready,
                ..
            }
        ) {
            break;
        }
    }
    let patch = h.snapshot(take.midi.unwrap());
    h.op(Operation::CandidateMidi {
        id,
        position: pinned.frames - 1,
        input: patch.id,
    });
    h.resource(ResourceRequest::ReleaseSnapshot { snapshot: patch.id });
    for _ in 0..128 {
        if matches!(
            h.op(Operation::CandidateStatus { id }),
            Reply::Candidate {
                status: CandidateStatus::Ready,
                ..
            }
        ) {
            break;
        }
    }
    assert!(matches!(
        h.op(Operation::CandidatePublish { id }),
        Reply::CandidatePublished { .. }
    ));
    assert_eq!(h.read(base.view.audio.unwrap().snapshot), pinned.audio);
    assert_eq!(h.read(base.view.midi.unwrap().snapshot), pinned.midi);
    for resource in [loaded.audio, loaded.midi].into_iter().flatten() {
        h.resource(ResourceRequest::Remove { resource });
    }
    let mut stopped = h.engine.render.into_stopped();
    recover_all(&mut stopped, 12288);
}

#[shoop_test]
fn fixed_shared_publication_round_trips_reload_edit_family_and_session_replacement() {
    let mut h = Fixture::new(32, 7);
    let (take, sibling, _) = h.grabs();
    h.adopt(0, take);
    h.adopt(1, sibling);
    let original = h.export_owned(0);
    let id = h.audio_candidate(take);
    let patch = h.snapshot(sibling.midi.unwrap());
    h.op(Operation::CandidateMidi {
        id,
        position: take.frames - 1,
        input: patch.id,
    });
    for _ in 0..128 {
        if matches!(
            h.op(Operation::CandidateStatus { id }),
            Reply::Candidate {
                status: CandidateStatus::Ready,
                ..
            }
        ) {
            break;
        }
    }
    let Reply::CandidatePublished {
        take: final_take, ..
    } = h.op(Operation::CandidatePublish { id })
    else {
        panic!("publish")
    };
    h.resource(ResourceRequest::ReleaseSnapshot { snapshot: patch.id });
    let final_export = h.export_owned(0);
    let old = Pinned::capture(&mut h, original);
    let final_data = Pinned::capture(&mut h, final_export);
    old.round_trip();
    final_data.round_trip();
    let loaded = final_data.loaded(&mut h, final_take);
    h.adopt(0, loaded);
    let Reply::Candidate {
        id,
        status: CandidateStatus::Building,
        ..
    } = h.op(Operation::CandidateBegin {
        target: target(0),
        take: loaded,
    })
    else {
        panic!("incremental loaded import")
    };
    for _ in 0..128 {
        if matches!(
            h.op(Operation::CandidateStatus { id }),
            Reply::Candidate {
                status: CandidateStatus::Ready,
                ..
            }
        ) {
            break;
        }
    }
    let patch = h.snapshot(sibling.audio.unwrap());
    h.op(Operation::CandidateAudio {
        id,
        position: take.frames - 1,
        input: patch.id,
        offset: 0,
        frames: 32,
    });
    h.resource(ResourceRequest::ReleaseSnapshot { snapshot: patch.id });
    let patch = h.snapshot(sibling.midi.unwrap());
    h.op(Operation::CandidateMidi {
        id,
        position: 1,
        input: patch.id,
    });
    h.resource(ResourceRequest::ReleaseSnapshot { snapshot: patch.id });
    for _ in 0..128 {
        if matches!(
            h.op(Operation::CandidateStatus { id }),
            Reply::Candidate {
                status: CandidateStatus::Ready,
                ..
            }
        ) {
            break;
        }
    }
    let Reply::CandidatePublished { take: reedited, .. } = h.op(Operation::CandidatePublish { id })
    else {
        panic!("loaded edit")
    };
    let reedited_export = h.export_owned(0);
    let reedited_data = Pinned::capture(&mut h, reedited_export);
    reedited_data.round_trip();
    assert_eq!(
        h.read(final_export.1.view.audio.unwrap().snapshot),
        final_data.audio
    );
    assert_eq!(
        h.read(final_export.1.view.midi.unwrap().snapshot),
        final_data.midi
    );

    h.request(protocol::Request::LoopSync {
        loop_id: protocol::LoopSlotId(protocol::SlotId::new(1, 1)),
        source: Some(protocol::LoopSlotId(protocol::SlotId::new(0, 1))),
    });
    let stale = h.audio_candidate(reedited);
    h.request(protocol::Request::ContentFamilyBegin {
        id: 1,
        root: protocol::LoopSlotId(protocol::SlotId::new(0, 1)),
        count: 2,
        sample_rate: 1000,
        quantum: 32,
    });
    for slot in 0..2 {
        let source = 1 + 2 * u64::from(slot);
        final_data.content(&mut h, source);
        let Reply::State(cut) = h.op(Operation::State {
            target: target(slot),
        }) else {
            panic!("family fence")
        };
        let outcome = h.request_outcome(protocol::Request::ContentFamilyEdit {
            id: 1,
            edit: protocol::ContentFamilyEdit {
                loop_id: protocol::LoopSlotId(protocol::SlotId::new(slot, 1)),
                sync_source: protocol::LoopSlotId(protocol::SlotId::new(0, 1)),
                expected_revision: cut.state.content_revision.0,
                expected_frames: take.frames,
                frames: take.frames,
                dry_audio: None,
                recorded_state: None,
                audio: Some(source),
                midi: Some(source + 1),
                replace: true,
                restart: slot == 0,
                fence_content: true,
            },
        });
        assert!(
            matches!(outcome, protocol::Outcome::ContentPrepared { .. }),
            "{outcome:?}"
        );
    }
    h.request(protocol::Request::ContentFamilyCommit { id: 1 });
    assert_eq!(
        h.op(Operation::CandidatePublish { id: stale }),
        failed(Error::StaleRevision)
    );
    h.op(Operation::CandidateCancel { id: stale });
    let retained = h.export(0);
    let displaced = h.audio_candidate(reedited);
    final_data.content(&mut h, 5);
    h.request(protocol::Request::ReplacementBegin { id: 1 });
    let strip = protocol::StripId(protocol::SlotId::new(0, 2));
    h.request(protocol::Request::ReplacementEdit {
        id: 1,
        edit: protocol::ReplacementEdit::Strip {
            strip,
            shape: protocol::StripShape {
                audio_inputs: 2,
                audio_outputs: 2,
                midi_input: true,
                midi_output: true,
            },
            controls: protocol::StripPreset {
                automatic_monitoring: false,
                input_gain: 1.,
                input_balance: 0.,
                monitoring: false,
                output_gain: 1.,
                output_balance: 0.,
                output_mute: false,
            },
        },
    });
    h.request(protocol::Request::ReplacementEdit {
        id: 1,
        edit: protocol::ReplacementEdit::Loop {
            dry_audio: None,
            recorded_state: None,
            loop_id: protocol::LoopSlotId(protocol::SlotId::new(0, 2)),
            strip,
            sync: None,
            frames: take.frames,
            audio: Some(5),
            midi: Some(6),
            gain: 0.75,
            balance: 0.,
        },
    });
    h.request(protocol::Request::ReplacementPrepare { id: 1 });
    h.request(protocol::Request::ReplacementCommit { id: 1 });
    assert!(matches!(
        h.op(Operation::CandidatePublish { id: displaced }),
        Reply::Failed(_)
    ));
    h.op(Operation::CandidateCancel { id: displaced });
    assert_eq!(
        h.read(retained.view.audio.unwrap().snapshot),
        final_data.audio
    );
    assert_eq!(h.read(original.1.view.midi.unwrap().snapshot), old.midi);
    assert_eq!(
        h.read(reedited_export.1.view.audio.unwrap().snapshot),
        reedited_data.audio
    );
    assert_eq!(
        h.read(reedited_export.1.view.midi.unwrap().snapshot),
        reedited_data.midi
    );
    for resource in [loaded.audio, loaded.midi].into_iter().flatten() {
        h.resource(ResourceRequest::Remove { resource });
    }
    let mut stopped = h.engine.render.into_stopped();
    recover_all(&mut stopped, 12288);
}

#[shoop_test]
fn long_loaded_writer_prepares_before_entry_and_publishes_after_one_tick_without_suffix_repair() {
    let q = 32;
    let mut h = Fixture::new_prepared_shapes(q, 7, 1000, true, true, true, true);
    let (stores, _, _) = h.grabs();
    let frames = 80 * q as u64;
    let mut initial = MidiState::default();
    initial.apply_packet(&[0xb0, 1, 13], 1024).unwrap();
    let mut pinned = Pinned {
        audio: (0..frames as usize * 2)
            .flat_map(|i| (i as f32 / 10.).to_le_bytes())
            .collect(),
        midi: Vec::new(),
        initial,
        packets: Vec::new(),
        frames,
    };
    for tick in 0..80 {
        let mut record = [0; 15];
        resource::encode_midi_record(tick * q as u64, &[0xb0, 2, tick as u8], &mut record).unwrap();
        pinned.midi.extend_from_slice(&record);
    }
    let loaded = pinned.loaded(&mut h, stores);
    h.adopt(0, loaded);
    let old = h.export(0);
    let Reply::History(history) = h.op(Operation::HistoryStatus { source: 0 }) else {
        panic!("source")
    };
    let at = h.aligned_future(128);
    let preparing = h.send(
        at,
        Operation::WriterBegin {
            target: target(0),
            source: history.id,
            take: loaded,
            kind: resource::indexed_midi::WriteKind::Fixed,
        },
    );
    let premature = h.send(
        at,
        Operation::Transition {
            target: target(0),
            timing: Timing::At,
            transition: Transition::ReplaceInput { position: None },
        },
    );
    let Reply::Writer {
        id,
        status: CandidateStatus::Building,
    } = h.result(preparing)
    else {
        panic!("bounded preparation")
    };
    assert!(matches!(
        h.result(premature),
        Reply::Transition(shoop_loops::Completion {
            outcome: shoop_loops::Outcome::Failed { .. },
            ..
        })
    ));
    loop {
        match h.op(Operation::WriterStatus { id }) {
            Reply::Writer {
                status: CandidateStatus::Building,
                ..
            } => {}
            Reply::Writer {
                status: CandidateStatus::Ready,
                ..
            } => break,
            reply => panic!("preparation: {reply:?}"),
        }
    }
    let start = h.aligned_future(128);
    let entry = h.send(
        start,
        Operation::Transition {
            target: target(0),
            timing: Timing::At,
            transition: Transition::ReplaceInput {
                position: Some(q as u64),
            },
        },
    );
    let exit = h.send(
        start + q as u64,
        Operation::Transition {
            target: target(0),
            timing: Timing::At,
            transition: Transition::Stop,
        },
    );
    assert!(matches!(
        h.result(entry),
        Reply::Transition(shoop_loops::Completion {
            outcome: shoop_loops::Outcome::Applied { .. },
            ..
        })
    ));
    assert!(matches!(
        h.result(exit),
        Reply::Transition(shoop_loops::Completion {
            outcome: shoop_loops::Outcome::Applied { .. },
            ..
        })
    ));
    let current = h.export(0);
    assert_eq!(current.view.description.frames, frames);
    let actual_audio = h.read(current.view.audio.unwrap().snapshot);
    let mut expected_audio = pinned.audio.clone();
    let mut expected_midi = midi_events(&pinned.midi);
    expected_midi.retain(|(at, _)| *at < q as u64 || *at >= 2 * q as u64);
    for offset in 0..q {
        let at = (q + offset) * 8;
        let sample = (start + offset as u64 + 1) as f32;
        expected_audio[at..at + 4].copy_from_slice(&sample.to_le_bytes());
        expected_audio[at + 4..at + 8].copy_from_slice(&(-2. * sample).to_le_bytes());
        for packet in packets(start + offset as u64, q) {
            expected_midi.push((q as u64 + offset as u64, packet));
        }
    }
    expected_midi.sort_by_key(|(at, _)| *at);
    assert_eq!(actual_audio, expected_audio);
    assert_eq!(
        midi_events(&h.read(current.view.midi.unwrap().snapshot)),
        expected_midi
    );
    assert_eq!(h.read(old.view.audio.unwrap().snapshot), pinned.audio);
    assert_eq!(h.read(old.view.midi.unwrap().snapshot), pinned.midi);
    for resource in [loaded.audio, loaded.midi].into_iter().flatten() {
        h.resource(ResourceRequest::Remove { resource });
    }
    let mut stopped = h.engine.render.into_stopped();
    recover_all(&mut stopped, 12288);
}
