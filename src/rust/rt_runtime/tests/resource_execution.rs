mod support;
use rt_runtime::buffer::{AudioBlock, MidiBlock, MidiBuffer};
use rt_runtime::executor::{Command, Control, GraphRuntime, PreparedNode, Retired};
use rt_runtime::graph::{compile, Connection, Endpoint, GraphDescription, PortCounts, PortType};
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::resource::*;
use rt_runtime::transport::{Completion, Lane, Outcome};
use rt_runtime::{
    Budgets, Config, Epoch, Error, Frame, NodeId, PreparedConfig, ResourceId, Revision,
};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn config() -> PreparedConfig {
    Config {
        max_frames: 4,
        audio_inputs: 0,
        audio_outputs: 1,
        midi_inputs: 0,
        midi_outputs: 0,
        max_nodes: 1,
        max_ports: 2,
        resource_slots: 16,
        resource_pages: 512,
        page_bytes: 64,
        resource_io_bytes: 32,
        command_slots: 4,
        budgets: Budgets {
            transfer_bytes: 128,
            validation_steps: 32,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()
    .unwrap()
}
fn id() -> NodeId {
    NodeId::from_parts(0, 1).unwrap()
}
fn graph(midi: bool) -> GraphDescription {
    let mut connections = vec![Connection {
        source: Endpoint {
            node: Some(id()),
            kind: PortType::Audio,
            port: 0,
        },
        destination: Endpoint {
            node: None,
            kind: PortType::Audio,
            port: 0,
        },
    }];
    if midi {
        connections.insert(
            0,
            Connection {
                source: Endpoint {
                    node: None,
                    kind: PortType::Midi,
                    port: 0,
                },
                destination: Endpoint {
                    node: Some(id()),
                    kind: PortType::Midi,
                    port: 0,
                },
            },
        );
    }
    GraphDescription {
        nodes: vec![id()],
        connections,
    }
}
fn install(store: &mut ResourceStore, resource: ResourceId, data: &[u8]) {
    let upload = store
        .begin_upload(
            resource,
            Revision(0),
            Description {
                format: Format::Audio { channels: 1 },
                frames: data.len() as u64 / 4,
                bytes: data.len(),
            },
        )
        .unwrap();
    for (index, chunk) in data.chunks(64).enumerate() {
        store.begin_callback();
        store.write_upload(upload, index * 64, chunk).unwrap();
    }
    store.finish_upload(upload).unwrap();
    store.commit(upload).unwrap();
}
fn floats(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

struct PlayerRecorder {
    playback: ResourceId,
    recording: ResourceId,
    snapshot: Snapshot,
    cursor: Cursor,
    rebind_refusals: usize,
}
impl Processor for PlayerRecorder {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_outputs: 1,
            ..PortCounts::default()
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_>) -> Result<(), Error> {
        if block.resources.info(self.playback)?.revision != self.snapshot.revision {
            if let Ok(snapshot) = block.resources.snapshot(self.playback, block.frame) {
                let cursor = block.resources.cursor(snapshot.id)?;
                block.resources.release_snapshot(self.snapshot.id)?;
                self.snapshot = snapshot;
                self.cursor = cursor;
            } else {
                self.rebind_refusals += 1;
            }
        }
        let mut encoded = [0u8; 16];
        let needed = block.frames * 4;
        let mut filled = 0;
        for _ in 0..block.frames * 2 + 1 {
            if filled == needed {
                break;
            }
            let count = block
                .resources
                .read(&mut self.cursor, &mut encoded[filled..needed])?;
            if count == 0 {
                self.cursor = block.resources.cursor(self.snapshot.id)?;
            }
            filled += count;
        }
        if filled != needed {
            return Err(Error::Incomplete);
        }
        let mut samples = [0f32; 4];
        for (sample, bytes) in samples[..block.frames]
            .iter_mut()
            .zip(encoded[..needed].chunks_exact(4))
        {
            *sample = f32::from_le_bytes(bytes.try_into().map_err(|_| Error::InvalidShape)?);
        }
        block
            .audio
            .output(0)
            .ok_or(Error::InvalidShape)?
            .copy_from_slice(&samples[..block.frames]);
        if let Ok(capture) = block.resources.active_capture(self.recording) {
            block
                .resources
                .capture_audio(capture, block.frame, &samples[..block.frames])?;
        }
        Ok(())
    }
}
struct Harness<P> {
    control: Control<P>,
    runtime: GraphRuntime<P>,
    config: PreparedConfig,
    expected: Option<f32>,
    last: [f32; 4],
    recording: bool,
    captured: Vec<f32>,
    inputs: Vec<MidiBuffer>,
    peak_copy: usize,
}
impl<P: Processor> Harness<P> {
    fn new(config: PreparedConfig, processor: P, store: ResourceStore, midi: bool) -> Self {
        let (control, runtime) = GraphRuntime::prepare_with_resources(
            config,
            vec![PreparedNode::new(id(), processor)],
            store,
        )
        .unwrap();
        let mut inputs = vec![];
        if midi {
            let mut input = MidiBuffer::new(1, 3).unwrap();
            input.push(0, 4, &[0x90, 60, 100]).unwrap();
            inputs.push(input);
        }
        let mut h = Self {
            control,
            runtime,
            config,
            expected: None,
            last: [0.; 4],
            recording: false,
            captured: vec![],
            inputs,
            peak_copy: 0,
        };
        let plan = compile(h.runtime.catalog(), &graph(midi)).unwrap();
        let completion = h.command(Command::commit(Epoch(0), plan));
        assert_eq!(completion.outcome, Outcome::Applied);
        h.expected = Some(1.0);
        assert_eq!(h.last, [1.; 4]);
        h
    }
    fn step(&mut self) {
        let frame = self.runtime.next_frame();
        let mut output = [0f32; 4];
        assert_no_heap(|| {
            self.runtime.process(
                AudioBlock {
                    frame,
                    frames: 4,
                    inputs: &[],
                    outputs: &mut [&mut output],
                },
                MidiBlock {
                    inputs: &self.inputs,
                    outputs: &mut [],
                },
            )
        })
        .unwrap();
        let diagnostics = self.runtime.diagnostics();
        assert!(diagnostics.transfer_bytes <= self.config.limits().budgets.transfer_bytes);
        assert!(diagnostics.validation_steps <= self.config.limits().budgets.validation_steps);
        assert_eq!(diagnostics.render_error, None);
        self.peak_copy = self.peak_copy.max(diagnostics.transfer_bytes);
        if let Some(value) = self.expected {
            assert_eq!(output, [value; 4], "frame {}", frame.0);
        }
        if self.recording {
            self.captured.extend_from_slice(&output);
        }
        self.last = output;
    }
    fn command(&mut self, command: Command<P>) -> Completion<Command<P>, Retired<P>> {
        self.control
            .submit(Lane::Normal, command)
            .map_err(|error| error.error)
            .unwrap();
        for _ in 0..128 {
            self.step();
            if let Some(completion) = self.control.try_completion() {
                return completion;
            }
        }
        panic!("bounded command did not complete");
    }
    fn request(&mut self, request: Request) -> ResourceCommand {
        let completion = self.command(Command::resource(request));
        assert_eq!(completion.outcome, Outcome::Applied);
        match completion.request {
            Command::Resource(command) => command,
            _ => panic!("wrong request"),
        }
    }
    fn upload(
        &mut self,
        resource: ResourceId,
        revision: Revision,
        description: Description,
        data: &[u8],
        chunk: usize,
    ) -> UploadId {
        let Some(Reply::Upload(upload)) = self
            .request(Request::BeginUpload {
                resource,
                expected_revision: revision,
                description,
            })
            .reply()
        else {
            panic!("missing upload");
        };
        for (index, bytes) in data.chunks(chunk).enumerate() {
            self.request(Request::Write {
                upload,
                offset: index * chunk,
                bytes: bytes.to_vec().into_boxed_slice(),
            });
        }
        self.request(Request::Finish { upload });
        upload
    }
    fn fetch(&mut self, snapshot: Snapshot) -> Vec<u8> {
        let Some(Reply::Cursor(mut cursor)) = self
            .request(Request::Cursor {
                snapshot: snapshot.id,
            })
            .reply()
        else {
            panic!("missing cursor");
        };
        let mut result = Vec::with_capacity(snapshot.description.bytes);
        let mut bytes = vec![0u8; 64].into_boxed_slice();
        for _ in 0..snapshot.description.bytes.div_ceil(64) + 1 {
            let command = self.request(Request::Read { cursor, bytes });
            let Some(Reply::Read { written, eof }) = command.reply() else {
                panic!("missing read");
            };
            let Request::Read {
                cursor: advanced,
                bytes: returned,
            } = command.into_request()
            else {
                panic!("missing buffer");
            };
            cursor = advanced;
            bytes = returned;
            result.extend_from_slice(&bytes[..written]);
            if eof {
                return result;
            }
        }
        panic!("export did not finish");
    }
}

#[shoop_test]
fn large_audio_midi_uploads_capture_and_exports_run_without_interrupting_resource_playback() {
    let config = config();
    let mut store = ResourceStore::new(config).unwrap();
    let playback = store.create().unwrap();
    let recording = store.create().unwrap();
    let original = floats(&vec![1.0; 2048]);
    let replacement = floats(&vec![2.0; 2048]);
    assert!(original.len() >= config.limits().budgets.transfer_bytes * 16);
    install(&mut store, playback, &original);
    let snapshot = store.snapshot(playback, Frame(0)).unwrap();
    let cursor = store.cursor(snapshot.id).unwrap();
    let mut h = Harness::new(
        config,
        PlayerRecorder {
            playback,
            recording,
            snapshot,
            cursor,
            rebind_refusals: 0,
        },
        store,
        false,
    );
    h.recording = true;
    let Some(Reply::Capture(capture)) = h
        .request(Request::BeginCapture {
            resource: recording,
            expected_revision: Revision(0),
            format: Format::Audio { channels: 1 },
        })
        .reply()
    else {
        panic!("missing capture");
    };
    let Some(Reply::Snapshot(old)) = h.request(Request::Snapshot { resource: playback }).reply()
    else {
        panic!("missing snapshot");
    };
    let upload = h.upload(
        playback,
        Revision(1),
        Description {
            format: Format::Audio { channels: 1 },
            frames: 2048,
            bytes: replacement.len(),
        },
        &replacement,
        64,
    );
    h.expected = Some(2.0);
    assert_eq!(
        h.request(Request::Commit { upload }).reply(),
        Some(Reply::Revision(Revision(2)))
    );
    assert_eq!(
        h.request(Request::Currentness { snapshot: old.id }).reply(),
        Some(Reply::Currentness(Currentness::Stale))
    );
    let Some(Reply::Snapshot(prefix)) = h
        .request(Request::Snapshot {
            resource: recording,
        })
        .reply()
    else {
        panic!("missing prefix");
    };
    assert!(prefix.capture.unwrap().active);
    assert_eq!(prefix.capture.unwrap().incomplete, None);
    assert_eq!(prefix.description.frames as usize, h.captured.len() - 4);
    let expected = floats(&h.captured[..prefix.description.frames as usize]);
    assert_eq!(h.fetch(prefix), expected);
    assert_eq!(
        h.request(Request::Currentness {
            snapshot: prefix.id
        })
        .reply(),
        Some(Reply::Currentness(Currentness::Stale))
    );
    h.recording = false;
    h.request(Request::StopCapture { capture });
    assert_eq!(h.fetch(old), original);
    h.request(Request::ReleaseSnapshot { snapshot: old.id });
    h.request(Request::ReleaseSnapshot {
        snapshot: prefix.id,
    });
    let Some(Reply::Created(midi_resource)) = h.request(Request::Create).reply() else {
        panic!("missing resource");
    };
    let mut midi_data = vec![];
    for index in 0..256 {
        let mut record = [0; 15];
        encode_midi_record(index * 2, &[0x90, 60, 100], &mut record).unwrap();
        midi_data.extend_from_slice(&record);
    }
    assert!(midi_data.len() >= config.limits().budgets.transfer_bytes * 16);
    let upload = h.upload(
        midi_resource,
        Revision(0),
        Description {
            format: Format::Midi,
            frames: 1024,
            bytes: midi_data.len(),
        },
        &midi_data,
        32,
    );
    h.request(Request::Commit { upload });
    let Some(Reply::Snapshot(midi_snapshot)) = h
        .request(Request::Snapshot {
            resource: midi_resource,
        })
        .reply()
    else {
        panic!("missing MIDI snapshot");
    };
    assert_eq!(h.fetch(midi_snapshot), midi_data);
    assert_eq!(midi_snapshot.description.frames, 1024);
    h.request(Request::ReleaseSnapshot {
        snapshot: midi_snapshot.id,
    });
    assert!(h.peak_copy >= 96);
    let empty = compile(
        h.runtime.catalog(),
        &GraphDescription {
            nodes: vec![],
            connections: vec![],
        },
    )
    .unwrap();
    h.expected = None;
    assert_eq!(
        h.command(Command::commit(h.runtime.epoch(), empty)).outcome,
        Outcome::Applied
    );
    h.expected = Some(0.0);
    let removal = h.command(Command::Remove { node: id() });
    let Some(Retired::Processor(player)) = removal.retired else {
        panic!("missing processor");
    };
    assert_eq!(player.rebind_refusals, 0);
    h.request(Request::ReleaseSnapshot {
        snapshot: player.snapshot.id,
    });
    for resource in [playback, recording, midi_resource] {
        h.request(Request::Remove { resource });
    }
    for _ in 0..128 {
        h.step();
        if h.runtime.resource_usage().retired_versions == 0
            && h.runtime.resource_usage().retired_pages == 0
        {
            break;
        }
    }
    assert_eq!(
        h.runtime.resource_usage(),
        PoolUsage {
            free_pages: 512,
            reserved_pages: 0,
            free_versions: 16,
            free_snapshots: 16,
            retired_versions: 0,
            retired_pages: 0
        }
    );
    assert_eq!(h.control.outstanding(), 0);
}

#[shoop_test]
fn lease_exhaustion_keeps_old_playback_until_a_new_revision_can_be_pinned() {
    let config = config();
    let mut store = ResourceStore::new(config).unwrap();
    let playback = store.create().unwrap();
    let recording = store.create().unwrap();
    install(&mut store, playback, &floats(&[1.0; 16]));
    let snapshot = store.snapshot(playback, Frame(0)).unwrap();
    let cursor = store.cursor(snapshot.id).unwrap();
    let mut h = Harness::new(
        config,
        PlayerRecorder {
            playback,
            recording,
            snapshot,
            cursor,
            rebind_refusals: 0,
        },
        store,
        false,
    );
    let mut leases = vec![];
    for _ in 0..15 {
        let Some(Reply::Snapshot(snapshot)) =
            h.request(Request::Snapshot { resource: playback }).reply()
        else {
            panic!("missing lease");
        };
        leases.push(snapshot.id);
    }
    assert_eq!(h.runtime.resource_usage().free_snapshots, 0);
    let upload = h.upload(
        playback,
        Revision(1),
        Description {
            format: Format::Audio { channels: 1 },
            frames: 16,
            bytes: 64,
        },
        &floats(&[2.0; 16]),
        64,
    );
    h.request(Request::Commit { upload });
    for _ in 0..8 {
        h.step();
    }
    h.expected = Some(2.0);
    h.request(Request::ReleaseSnapshot {
        snapshot: leases.pop().unwrap(),
    });
    for snapshot in leases {
        h.request(Request::ReleaseSnapshot { snapshot });
    }
    let empty = compile(
        h.runtime.catalog(),
        &GraphDescription {
            nodes: vec![],
            connections: vec![],
        },
    )
    .unwrap();
    h.expected = None;
    h.command(Command::commit(h.runtime.epoch(), empty));
    h.expected = Some(0.0);
    let removal = h.command(Command::Remove { node: id() });
    let Some(Retired::Processor(player)) = removal.retired else {
        panic!("missing player");
    };
    assert!(player.rebind_refusals >= 9);
    h.request(Request::ReleaseSnapshot {
        snapshot: player.snapshot.id,
    });
    h.request(Request::Remove { resource: playback });
    h.request(Request::Remove {
        resource: recording,
    });
    for _ in 0..8 {
        h.step();
    }
    assert_eq!(h.runtime.resource_usage().free_pages, 512);
    assert_eq!(h.runtime.resource_usage().free_versions, 16);
    assert_eq!(h.runtime.resource_usage().free_snapshots, 16);
}

struct MidiRecorder {
    recording: ResourceId,
}
impl Processor for MidiRecorder {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_outputs: 1,
            midi_inputs: 1,
            ..PortCounts::default()
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_>) -> Result<(), Error> {
        block.audio.output(0).ok_or(Error::InvalidShape)?.fill(1.0);
        if let Ok(capture) = block.resources.active_capture(self.recording) {
            block.resources.capture_midi(
                capture,
                block.frame,
                block.frames,
                &block.midi_inputs[0],
            )?;
        }
        Ok(())
    }
}
#[shoop_test]
fn background_midi_validation_cannot_spend_reserved_processor_capture_steps() {
    let config = Config {
        midi_inputs: 1,
        max_ports: 4,
        resource_io_bytes: 16,
        resource_io_steps: 1,
        budgets: Budgets {
            validation_steps: 4,
            ..config().limits().budgets
        },
        ..config().limits()
    }
    .prepare()
    .unwrap();
    let mut store = ResourceStore::new(config).unwrap();
    let recording = store.create().unwrap();
    let background = store.create().unwrap();
    let mut h = Harness::new(config, MidiRecorder { recording }, store, true);
    h.request(Request::BeginCapture {
        resource: recording,
        expected_revision: Revision(0),
        format: Format::Midi,
    });
    let mut record = [0; 15];
    encode_midi_record(0, &[0x90, 64, 100], &mut record).unwrap();
    let Some(Reply::Upload(upload)) = h
        .request(Request::BeginUpload {
            resource: background,
            expected_revision: Revision(0),
            description: Description {
                format: Format::Midi,
                frames: 1,
                bytes: 15,
            },
        })
        .reply()
    else {
        panic!("missing upload");
    };
    for (index, bytes) in record.chunks(3).enumerate() {
        h.request(Request::Write {
            upload,
            offset: index * 3,
            bytes: bytes.to_vec().into_boxed_slice(),
        });
        assert_eq!(h.runtime.diagnostics().validation_steps, 4);
        assert_eq!(h.runtime.diagnostics().transfer_bytes, 18);
    }
    h.request(Request::Finish { upload });
    h.request(Request::Commit { upload });
    let Some(Reply::Info(info)) = h
        .request(Request::Info {
            resource: recording,
        })
        .reply()
    else {
        panic!("missing info");
    };
    assert!(info.capture.unwrap().active);
    assert_eq!(info.capture.unwrap().incomplete, None);
}

struct Passive;
impl Processor for Passive {
    fn ports(&self) -> PortCounts {
        PortCounts::default()
    }
    fn process(&mut self, _: &mut ProcessBlock<'_>) -> Result<(), Error> {
        Ok(())
    }
}
fn empty_cycle(runtime: &mut GraphRuntime<Passive>) {
    let frame = runtime.next_frame();
    let mut output = [0f32; 4];
    assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame,
                frames: 4,
                inputs: &[],
                outputs: &mut [&mut output],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
    })
    .unwrap();
}

#[shoop_test]
fn resource_payloads_survive_saturation_refusal_cancellation_recovery_and_disconnect() {
    let config = Config {
        command_slots: 2,
        budgets: Budgets {
            commands: 2,
            publications: 2,
            ..config().limits().budgets
        },
        ..config().limits()
    }
    .prepare()
    .unwrap();
    let mut store = ResourceStore::new(config).unwrap();
    let resource = store.create().unwrap();
    let upload = store
        .begin_upload(
            resource,
            Revision(0),
            Description {
                format: Format::Audio { channels: 1 },
                frames: 32,
                bytes: 128,
            },
        )
        .unwrap();
    let readable = store.create().unwrap();
    install(&mut store, readable, &[7; 64]);
    let snapshot = store.snapshot(readable, Frame(0)).unwrap();
    let _held_until_shutdown = store.snapshot(readable, Frame(0)).unwrap();
    let cursor = store.cursor(snapshot.id).unwrap();
    let (mut control, mut runtime) =
        GraphRuntime::<Passive>::prepare_with_resources(config, vec![], store).unwrap();
    for (offset, value) in [(0, 1), (64, 2)] {
        control
            .submit(
                Lane::Normal,
                Command::resource(Request::Write {
                    upload,
                    offset,
                    bytes: vec![value; 64].into_boxed_slice(),
                }),
            )
            .map_err(|e| e.error)
            .unwrap();
    }
    let refusal = control
        .submit(Lane::Normal, Command::resource(Request::Info { resource }))
        .err()
        .unwrap();
    assert_eq!(refusal.error, Error::Full);
    for _ in 0..16 {
        empty_cycle(&mut runtime);
    }
    assert_eq!(control.outstanding(), 2);
    let first = control.try_completion().unwrap();
    let second = control.try_completion().unwrap();
    assert_eq!(first.outcome, Outcome::Applied);
    assert_eq!(second.outcome, Outcome::Rejected(Error::Budget));
    let Command::Resource(ref operation) = second.request else {
        panic!("missing payload");
    };
    assert_eq!(operation.reply(), None);
    let Request::Write { bytes, .. } = operation.request() else {
        panic!("missing bytes");
    };
    assert_eq!(&**bytes, &[2; 64]);
    control
        .submit(Lane::Normal, second.request)
        .map_err(|e| e.error)
        .unwrap();
    empty_cycle(&mut runtime);
    assert_eq!(control.try_completion().unwrap().outcome, Outcome::Applied);
    for request in [Request::Finish { upload }, Request::Commit { upload }] {
        control
            .submit(Lane::Normal, Command::resource(request))
            .map_err(|e| e.error)
            .unwrap();
        empty_cycle(&mut runtime);
        assert_eq!(control.try_completion().unwrap().outcome, Outcome::Applied);
    }
    control
        .submit(
            Lane::Normal,
            Command::resource(Request::Read {
                cursor,
                bytes: vec![0xcd; 64].into_boxed_slice(),
            }),
        )
        .map_err(|e| e.error)
        .unwrap();
    control
        .submit(Lane::Normal, Command::resource(Request::Info { resource }))
        .map_err(|e| e.error)
        .unwrap();
    control
        .submit(
            Lane::Recovery,
            Command::resource(Request::ReleaseSnapshot {
                snapshot: snapshot.id,
            }),
        )
        .map_err(|e| e.error)
        .unwrap();
    empty_cycle(&mut runtime);
    let release = control.try_completion().unwrap();
    assert_eq!(release.lane, Lane::Recovery);
    assert_eq!(release.outcome, Outcome::Applied);
    let read = control.try_completion().unwrap();
    assert_eq!(read.outcome, Outcome::Rejected(Error::InvalidId));
    let Command::Resource(read) = read.request else {
        panic!("missing read");
    };
    let Request::Read { bytes, .. } = read.into_request() else {
        panic!("missing read buffer");
    };
    assert_eq!(&*bytes, &[0xcd; 64]);
    empty_cycle(&mut runtime);
    assert_eq!(control.try_completion().unwrap().outcome, Outcome::Applied);
    // Reusing a completed request clears its old reply even if cancelled before dispatch.
    let ticket = control
        .submit(Lane::Normal, first.request)
        .map_err(|e| e.error)
        .unwrap();
    ticket.cancel();
    empty_cycle(&mut runtime);
    let cancelled = control.try_completion().unwrap();
    assert_eq!(cancelled.outcome, Outcome::Cancelled);
    let Command::Resource(cancelled) = cancelled.request else {
        panic!("missing cancelled payload");
    };
    assert_eq!(cancelled.reply(), None);
    let Request::Write { bytes, .. } = cancelled.into_request() else {
        panic!("missing cancelled bytes");
    };
    assert_eq!(&*bytes, &[1; 64]);
    let oversized = control
        .submit(
            Lane::Normal,
            Command::resource(Request::Read {
                cursor,
                bytes: vec![0; 65].into_boxed_slice(),
            }),
        )
        .err()
        .unwrap();
    assert_eq!(oversized.error, Error::Capacity);
    assert_eq!(control.outstanding(), 0);
    let mut sysex = vec![0; 128];
    sysex[0] = 0xf0;
    sysex[127] = 0xf7;
    control
        .submit(
            Lane::Normal,
            Command::Schedule {
                at: runtime.next_frame(),
                action: rt_runtime::schedule::Action::Midi {
                    node: id(),
                    port: 0,
                },
                payload: sysex.into_boxed_slice(),
            },
        )
        .map_err(|e| e.error)
        .unwrap();
    control
        .submit(Lane::Normal, Command::resource(Request::Info { resource }))
        .map_err(|e| e.error)
        .unwrap();
    empty_cycle(&mut runtime);
    assert_eq!(
        control.try_completion().unwrap().outcome,
        Outcome::Rejected(Error::Capacity)
    );
    assert_eq!(control.try_completion().unwrap().outcome, Outcome::Applied);
    control
        .submit(
            Lane::Normal,
            Command::resource(Request::Read {
                cursor,
                bytes: vec![0; 64].into_boxed_slice(),
            }),
        )
        .map_err(|e| e.error)
        .unwrap();
    drop(control);
    for _ in 0..16 {
        empty_cycle(&mut runtime);
    }
    // Final fixed-pool/queued-owner destruction occurs here, outside the guard.
}

#[shoop_test]
fn resource_io_reservations_leave_control_progress_and_cannot_reset_through_processor_views() {
    let limits = config().limits();
    assert_eq!(
        Config {
            resource_io_bytes: 65,
            ..limits
        }
        .prepare(),
        Err(Error::InvalidConfig)
    );
    assert_eq!(
        Config {
            resource_io_steps: 32,
            ..limits
        }
        .prepare(),
        Err(Error::InvalidConfig)
    );
    let mut store = ResourceStore::new(config()).unwrap();
    let resource = store.create().unwrap();
    install(&mut store, resource, &[0; 64]);
    let snapshot = store.snapshot(resource, Frame(0)).unwrap();
    let mut cursor = store.cursor(snapshot.id).unwrap();
    let mut bytes = [0; 64];
    assert_no_heap(|| {
        store.begin_callback();
        store.io().read(&mut cursor, &mut bytes).unwrap();
        let mut other_cursor = store.io().cursor(snapshot.id).unwrap();
        store.io().read(&mut other_cursor, &mut bytes).unwrap();
        let mut third_cursor = store.io().cursor(snapshot.id).unwrap();
        assert_eq!(
            store.io().read(&mut third_cursor, &mut bytes),
            Err(Error::Budget)
        );
        assert_eq!(store.work().copied_bytes, 128);
    });
}
