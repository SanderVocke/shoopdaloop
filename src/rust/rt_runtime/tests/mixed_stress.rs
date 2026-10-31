mod support;
use rt_runtime::buffer::{AudioBlock, MidiBlock, MidiBuffer};
use rt_runtime::executor::{Command, Control, GraphRuntime, PreparedNode, Retired};
use rt_runtime::graph::{compile, Connection, Endpoint, GraphDescription, PortCounts, PortType};
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::resource::*;
use rt_runtime::schedule::Action;
use rt_runtime::transport::{Completion, Lane, Outcome};
use rt_runtime::{Budgets, Config, Epoch, Error, Frame, NodeId, ResourceId, Revision, Sequence};
use shoop_wasm_test_support::shoop_test;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn node_id() -> NodeId {
    NodeId::from_parts(0, 1).unwrap()
}
struct Random(u32);
impl Random {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
}
struct Signal {
    playback: ResourceId,
    recording: ResourceId,
    snapshot: Snapshot,
    cursor: Cursor,
    gain: f32,
    frames: u64,
    drops: Arc<AtomicUsize>,
}
impl Drop for Signal {
    fn drop(&mut self) {
        assert!(!support::is_rendering());
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}
impl Processor for Signal {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_outputs: 2,
            midi_inputs: 1,
            midi_outputs: 1,
            ..PortCounts::default()
        }
    }
    fn set_parameter(&mut self, parameter: u32, value: f32) -> Result<(), Error> {
        if parameter != 0 || !value.is_finite() {
            return Err(Error::InvalidParameter);
        }
        self.gain = value;
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_>) -> Result<(), Error> {
        if block.resources.info(self.playback)?.revision != self.snapshot.revision {
            let next = block.resources.snapshot(self.playback, block.frame)?;
            let cursor = block.resources.cursor(next.id)?;
            block.resources.release_snapshot(self.snapshot.id)?;
            self.snapshot = next;
            self.cursor = cursor;
        }
        let mut samples = [0.; 4];
        for (offset, sample) in samples[..block.frames].iter_mut().enumerate() {
            let mut bytes = [0u8; 4];
            if block.resources.read(&mut self.cursor, &mut bytes)? == 0 {
                self.cursor = block.resources.cursor(self.snapshot.id)?;
                if block.resources.read(&mut self.cursor, &mut bytes)? != 4 {
                    return Err(Error::Incomplete);
                }
            }
            *sample = f32::from_le_bytes(bytes) * self.gain
                + (self.frames + offset as u64) as f32 / 65536.;
        }
        block
            .audio
            .output(0)
            .ok_or(Error::InvalidShape)?
            .copy_from_slice(&samples[..block.frames]);
        for (out, sample) in block
            .audio
            .output(1)
            .ok_or(Error::InvalidShape)?
            .iter_mut()
            .zip(&samples)
        {
            *out = -*sample;
        }
        if let Ok(capture) = block.resources.active_capture(self.recording) {
            match block
                .resources
                .capture_audio(capture, block.frame, &samples[..block.frames])
            {
                Ok(_) | Err(Error::Capacity | Error::Budget) => {}
                Err(error) => return Err(error),
            }
        }
        for index in 0..block.midi_inputs[0].len() {
            let (offset, packet) = block.midi_inputs[0].get(index).ok_or(Error::InvalidEvent)?;
            match block.midi_outputs[0].push(offset, block.frames, packet) {
                Ok(()) => {}
                Err(Error::Full) => break,
                Err(error) => return Err(error),
            }
        }
        self.frames = self
            .frames
            .checked_add(block.frames as u64)
            .ok_or(Error::Exhausted)?;
        Ok(())
    }
}
struct Lease {
    snapshot: Snapshot,
    cursor: Cursor,
    expected: Vec<u8>,
    read: usize,
}
struct Host {
    control: Option<Control<Signal>>,
    runtime: GraphRuntime<Signal>,
    playback: ResourceId,
    recording: ResourceId,
    file_value: f32,
    file_revision: Revision,
    capture_revision: Revision,
    capture: Option<CaptureId>,
    capture_start: u64,
    captured: Vec<f32>,
    last_source: [f32; 4],
    leases: [Option<Lease>; 4],
    sign: f32,
    gain: f32,
    parameters: Vec<(u64, Sequence, f32)>,
    midi: Vec<(u64, Sequence, [u8; 3])>,
    scheduled: Vec<(Sequence, u64)>,
    midi_output: [MidiBuffer; 1],
    recovery: MidiBuffer,
    reset: bool,
    peak_owners: usize,
    peak_pages: usize,
    callbacks: usize,
    replacements: usize,
    snapshots: usize,
    stalls: usize,
}
impl Host {
    fn new(drops: Arc<AtomicUsize>) -> Self {
        let config = Config {
            max_frames: 4,
            audio_inputs: 0,
            audio_outputs: 1,
            midi_inputs: 0,
            midi_outputs: 1,
            max_nodes: 1,
            max_ports: 6,
            max_edges: 4,
            command_slots: 4,
            midi_events_per_port: 8,
            midi_bytes_per_port: 64,
            resource_slots: 16,
            resource_pages: 128,
            page_bytes: 64,
            resource_io_bytes: 32,
            budgets: Budgets {
                commands: 5,
                publications: 5,
                transfer_bytes: 128,
                validation_steps: 128,
                ..Budgets::default()
            },
            ..Config::default()
        }
        .prepare()
        .unwrap();
        let mut store = ResourceStore::new(config).unwrap();
        let playback = store.create().unwrap();
        let recording = store.create().unwrap();
        let id = store
            .begin_upload(
                playback,
                Revision(0),
                Description {
                    format: Format::Audio { channels: 1 },
                    frames: 1,
                    bytes: 4,
                },
            )
            .unwrap();
        store.begin_callback();
        store.write_upload(id, 0, &1f32.to_le_bytes()).unwrap();
        store.finish_upload(id).unwrap();
        store.commit(id).unwrap();
        let snapshot = store.snapshot(playback, Frame(0)).unwrap();
        let cursor = store.cursor(snapshot.id).unwrap();
        let node = Signal {
            playback,
            recording,
            snapshot,
            cursor,
            gain: 1.,
            frames: 0,
            drops,
        };
        let (control, runtime) = GraphRuntime::prepare_with_resources(
            config,
            vec![PreparedNode::new(node_id(), node)],
            store,
        )
        .unwrap();
        let mut host = Self {
            control: Some(control),
            runtime,
            playback,
            recording,
            file_value: 1.,
            file_revision: Revision(1),
            capture_revision: Revision(0),
            capture: None,
            capture_start: 0,
            captured: Vec::new(),
            last_source: [0.; 4],
            leases: std::array::from_fn(|_| None),
            sign: 1.,
            gain: 1.,
            parameters: Vec::new(),
            midi: Vec::new(),
            scheduled: Vec::new(),
            midi_output: [MidiBuffer::new(8, 64).unwrap()],
            recovery: MidiBuffer::new(32, 96).unwrap(),
            reset: false,
            peak_owners: 0,
            peak_pages: 0,
            callbacks: 0,
            replacements: 0,
            snapshots: 0,
            stalls: 0,
        };
        host.rewire(false, false);
        host
    }
    fn plan(&self, negative: bool, empty: bool) -> rt_runtime::graph::PreparedPlan {
        let graph = if empty {
            GraphDescription::default()
        } else {
            GraphDescription {
                nodes: vec![node_id()],
                connections: vec![
                    Connection {
                        source: Endpoint::node(node_id(), PortType::Audio, u32::from(negative)),
                        destination: Endpoint::host(PortType::Audio, 0),
                    },
                    Connection {
                        source: Endpoint::node(node_id(), PortType::Midi, 0),
                        destination: Endpoint::host(PortType::Midi, 0),
                    },
                ],
            }
        };
        compile(self.runtime.catalog(), &graph).unwrap()
    }
    fn pump(&mut self) {
        let frame = self.runtime.next_frame();
        let mut audio = [0.; 4];
        assert_no_heap(|| {
            self.runtime
                .process(
                    AudioBlock {
                        frame,
                        frames: 4,
                        inputs: &[],
                        outputs: &mut [&mut audio],
                    },
                    MidiBlock {
                        inputs: &[],
                        outputs: &mut self.midi_output,
                    },
                )
                .unwrap();
            self.recovery.clear();
            self.runtime
                .drain_midi_recovery(0, &mut self.recovery, 4)
                .unwrap();
        });
        let diagnostics = self.runtime.diagnostics();
        assert_eq!(diagnostics.render_error, None);
        assert!(
            diagnostics.transfer_bytes <= 128
                && diagnostics.validation_steps <= 128
                && diagnostics.subranges <= 8
        );
        self.parameters.sort_by_key(|entry| (entry.0, entry.1));
        self.midi.sort_by_key(|entry| (entry.0, entry.1));
        for (offset, actual) in audio.iter().enumerate() {
            let at = frame.0 + offset as u64;
            while self.parameters.first().is_some_and(|entry| entry.0 <= at) {
                self.gain = self.parameters.remove(0).2;
            }
            let value = self.file_value * self.gain + at as f32 / 65536.;
            assert_eq!(
                *actual,
                if self.sign == 0. {
                    0.
                } else {
                    self.sign * value
                },
                "frame {at}"
            );
            self.last_source[offset] = value;
            if self.capture.is_some() {
                self.captured.push(value);
            }
        }
        let due = self
            .midi
            .iter()
            .take_while(|entry| entry.0 < frame.0 + 4)
            .count();
        assert_eq!(self.midi_output[0].len(), due);
        for (index, (at, _, packet)) in self.midi[..due].iter().enumerate() {
            let (offset, actual) = self.midi_output[0].get(index).unwrap();
            assert_eq!(frame.0 + offset as u64, *at);
            assert_eq!(actual, packet);
        }
        self.midi.drain(..due);
        if self.reset {
            assert_eq!(self.recovery.len(), 32);
            for index in 0..32 {
                let (offset, data) = self.recovery.get(index).unwrap();
                assert_eq!(offset, 0);
                assert_eq!(
                    data,
                    &[
                        0xb0 | (index / 2) as u8,
                        if index % 2 == 0 { 120 } else { 123 },
                        0
                    ]
                );
            }
            self.reset = false;
        } else {
            assert!(self.recovery.is_empty());
        }
        if let Some(control) = &mut self.control {
            while let Some(result) = control.try_execution() {
                let position = self
                    .scheduled
                    .iter()
                    .position(|entry| entry.0 == result.sequence)
                    .unwrap();
                let (_, expected) = self.scheduled.remove(position);
                assert_eq!(result.actual_frame, Frame(expected));
                assert_eq!(result.result, Ok(()));
            }
            self.peak_owners = self.peak_owners.max(control.outstanding());
            assert!(control.outstanding() <= 5);
        }
        let usage = self.runtime.resource_usage();
        self.peak_pages = self.peak_pages.max(128 - usage.free_pages);
        assert!(usage.free_pages <= 128 && usage.reserved_pages <= usage.free_pages);
        self.callbacks += 1;
    }
    fn command(
        &mut self,
        command: Command<Signal>,
        lane: Lane,
    ) -> Completion<Command<Signal>, Retired<Signal>> {
        self.control
            .as_mut()
            .unwrap()
            .submit(lane, command)
            .map_err(|refusal| refusal.error)
            .unwrap();
        for _ in 0..64 {
            self.pump();
            if let Some(completion) = self.control.as_mut().unwrap().try_completion() {
                return completion;
            }
        }
        panic!("finite command failed to complete");
    }
    fn resource(
        &mut self,
        request: Request,
        lane: Lane,
    ) -> Completion<Command<Signal>, Retired<Signal>> {
        self.command(Command::resource(request), lane)
    }
    fn reply(completion: &Completion<Command<Signal>, Retired<Signal>>) -> Reply {
        assert_eq!(completion.outcome, Outcome::Applied);
        let Command::Resource(command) = &completion.request else {
            panic!("resource reply")
        };
        command.reply().unwrap()
    }
    fn rewire(&mut self, negative: bool, stale: bool) {
        let plan = self.plan(negative, false);
        let epoch = self.runtime.epoch();
        if !stale {
            self.sign = if negative { -1. } else { 1. };
        }
        let completion = self.command(
            Command::commit(if stale { Epoch(epoch.0 + 1) } else { epoch }, plan),
            Lane::Normal,
        );
        assert_eq!(
            completion.outcome,
            if stale {
                Outcome::Rejected(Error::StaleEpoch)
            } else {
                Outcome::Applied
            }
        );
    }
    fn schedule(&mut self, value: u32, midi: bool, late: bool) {
        let at = if late {
            Frame(0)
        } else {
            self.runtime
                .next_frame()
                .checked_add(value as usize % 8)
                .unwrap()
        };
        let effective = at.0.max(self.runtime.next_frame().0);
        let gain = [0.5, 1., 2., 4.][value as usize % 4];
        let packet = [
            if value & 1 == 0 { 0x90 } else { 0x80 },
            60,
            if value & 1 == 0 { 100 } else { 0 },
        ];
        let action = if midi {
            Action::Midi {
                node: node_id(),
                port: 0,
            }
        } else {
            Action::Parameter {
                node: node_id(),
                parameter: 0,
                value: gain,
            }
        };
        let payload = if midi {
            packet.to_vec().into_boxed_slice()
        } else {
            Box::default()
        };
        let ticket = self
            .control
            .as_mut()
            .unwrap()
            .submit(
                Lane::Normal,
                Command::Schedule {
                    at,
                    action,
                    payload,
                },
            )
            .map_err(|refusal| refusal.error)
            .unwrap();
        let sequence = ticket.sequence();
        self.scheduled.push((sequence, effective));
        if midi {
            self.midi.push((effective, sequence, packet));
        } else {
            self.parameters.push((effective, sequence, gain));
        }
        self.pump();
        let completion = self.control.as_mut().unwrap().try_completion().unwrap();
        assert_eq!(
            completion.outcome,
            Outcome::Scheduled {
                effective_frame: Frame(effective),
                late_by: effective - at.0
            }
        );
    }
    fn replace(&mut self, value: f32) {
        let description = Description {
            format: Format::Audio { channels: 1 },
            frames: 32,
            bytes: 128,
        };
        let started = self.resource(
            Request::BeginUpload {
                resource: self.playback,
                expected_revision: self.file_revision,
                description,
            },
            Lane::Normal,
        );
        if started.outcome == Outcome::Rejected(Error::Capacity) {
            return;
        }
        let Reply::Upload(upload) = Self::reply(&started) else {
            panic!("upload")
        };
        for offset in [0, 64] {
            let bytes = (0..16)
                .flat_map(|_| value.to_le_bytes())
                .collect::<Vec<_>>()
                .into_boxed_slice();
            Self::reply(&self.resource(
                Request::Write {
                    upload,
                    offset,
                    bytes,
                },
                Lane::Normal,
            ));
        }
        Self::reply(&self.resource(Request::Finish { upload }, Lane::Normal));
        self.file_value = value;
        let done = self.resource(Request::Commit { upload }, Lane::Normal);
        self.file_revision = Revision(self.file_revision.0 + 1);
        assert_eq!(Self::reply(&done), Reply::Revision(self.file_revision));
        self.replacements += 1;
    }
    fn toggle_capture(&mut self) {
        if let Some(capture) = self.capture.take() {
            Self::reply(&self.resource(Request::StopCapture { capture }, Lane::Recovery));
        } else {
            let observed = self.resource(
                Request::Info {
                    resource: self.recording,
                },
                Lane::Normal,
            );
            if observed.outcome == Outcome::Applied {
                let Reply::Info(info) = Self::reply(&observed) else {
                    panic!("capture metadata")
                };
                self.capture_revision = info.revision;
            } else {
                assert_eq!(observed.outcome, Outcome::Rejected(Error::InvalidState));
            }
            let completion = self.resource(
                Request::BeginCapture {
                    resource: self.recording,
                    expected_revision: self.capture_revision,
                    format: Format::Audio { channels: 1 },
                },
                Lane::Normal,
            );
            if completion.outcome == Outcome::Rejected(Error::Capacity) {
                return;
            }
            let Reply::Capture(capture) = Self::reply(&completion) else {
                panic!("capture")
            };
            self.capture = Some(capture);
            self.capture_start = completion.frame.0;
            self.captured.clear();
            self.captured.extend_from_slice(&self.last_source);
        }
    }
    fn snapshot(&mut self, index: usize) {
        if let Some(lease) = self.leases[index].take() {
            Self::reply(&self.resource(
                Request::ReleaseSnapshot {
                    snapshot: lease.snapshot.id,
                },
                Lane::Recovery,
            ));
        }
        let before = self.captured.len();
        let result = self.resource(
            Request::Snapshot {
                resource: self.recording,
            },
            Lane::Normal,
        );
        if matches!(
            result.outcome,
            Outcome::Rejected(Error::Capacity | Error::InvalidState)
        ) {
            return;
        }
        let Reply::Snapshot(snapshot) = Self::reply(&result) else {
            panic!("snapshot")
        };
        let info = snapshot.capture.unwrap();
        assert_eq!(snapshot.cut_frame.0, info.next_frame.0);
        assert_eq!(
            snapshot.description.frames,
            info.next_frame.0 - info.start_frame.0
        );
        assert_eq!(info.start_frame.0, self.capture_start);
        assert!(snapshot.description.frames as usize <= before);
        if (snapshot.description.frames as usize) < before {
            assert!(info.incomplete.is_some());
        }
        let expected = self.captured[..snapshot.description.frames as usize]
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect();
        let cursor = Self::reply(&self.resource(
            Request::Cursor {
                snapshot: snapshot.id,
            },
            Lane::Normal,
        ));
        let Reply::Cursor(cursor) = cursor else {
            panic!("cursor")
        };
        self.leases[index] = Some(Lease {
            snapshot,
            cursor,
            expected,
            read: 0,
        });
        self.snapshots += 1;
    }
    fn read(&mut self, index: usize) {
        let Some(mut lease) = self.leases[index].take() else {
            self.pump();
            return;
        };
        let response = self.resource(
            Request::Read {
                cursor: lease.cursor,
                bytes: vec![0xab; 64].into_boxed_slice(),
            },
            Lane::Normal,
        );
        let Reply::Read { written, eof } = Self::reply(&response) else {
            panic!("read")
        };
        let Command::Resource(command) = &response.request else {
            panic!("resource")
        };
        let Request::Read { cursor, bytes } = command.request() else {
            panic!("read request")
        };
        assert_eq!(
            &bytes[..written],
            &lease.expected[lease.read..lease.read + written]
        );
        lease.read += written;
        lease.cursor = *cursor;
        assert_eq!(eof, lease.read == lease.expected.len());
        assert_eq!(cursor.position(), lease.read);
        self.leases[index] = Some(lease);
    }
    fn stall(&mut self) {
        for _ in 0..3 {
            self.pump();
        }
        assert!(self.midi.is_empty() && self.parameters.is_empty() && self.scheduled.is_empty());
        let negative = self.sign > 0.;
        let plan = self.plan(negative, false);
        let epoch = self.runtime.epoch();
        self.sign = -self.sign;
        let mut expected = Vec::new();
        for command in [
            Command::commit(epoch, plan),
            Command::resource(Request::Write {
                upload: UploadId {
                    slot: u32::MAX,
                    generation: 1,
                },
                offset: 0,
                bytes: vec![7; 64].into_boxed_slice(),
            }),
            Command::resource(Request::Info {
                resource: self.playback,
            }),
            Command::resource(Request::Info {
                resource: self.playback,
            }),
        ] {
            let ticket = self
                .control
                .as_mut()
                .unwrap()
                .submit(Lane::Normal, command)
                .map_err(|refusal| refusal.error)
                .unwrap();
            let cancelled = expected.len() == 1;
            if cancelled {
                ticket.cancel();
            }
            expected.push((
                ticket.sequence(),
                if cancelled {
                    Outcome::Cancelled
                } else {
                    Outcome::Applied
                },
            ));
        }
        let refusal = self
            .control
            .as_mut()
            .unwrap()
            .submit(
                Lane::Normal,
                Command::resource(Request::Write {
                    upload: UploadId {
                        slot: u32::MAX,
                        generation: 1,
                    },
                    offset: 0,
                    bytes: vec![9; 64].into_boxed_slice(),
                }),
            )
            .err()
            .unwrap();
        assert_eq!(refusal.error, Error::Full);
        let Command::Resource(refused) = refusal.value else {
            panic!("refused owner")
        };
        let Request::Write { bytes, .. } = refused.into_request() else {
            panic!("refused buffer")
        };
        assert_eq!(&*bytes, &[9; 64]);
        let ticket = self
            .control
            .as_mut()
            .unwrap()
            .submit(Lane::Recovery, Command::ResetMidi)
            .map_err(|refusal| refusal.error)
            .unwrap();
        expected.push((ticket.sequence(), Outcome::Applied));
        self.reset = true;
        for _ in 0..5 {
            self.pump();
            assert_eq!(self.control.as_ref().unwrap().outstanding(), 5);
        }
        while let Some(completion) = self.control.as_mut().unwrap().try_completion() {
            let index = expected
                .iter()
                .position(|entry| entry.0 == completion.sequence)
                .unwrap();
            assert_eq!(completion.outcome, expected.remove(index).1);
            if completion.outcome == Outcome::Cancelled {
                let Command::Resource(command) = completion.request else {
                    panic!("owning cancellation")
                };
                let Request::Write { bytes, .. } = command.into_request() else {
                    panic!("cancelled buffer")
                };
                assert_eq!(&*bytes, &[7; 64]);
            }
        }
        assert!(expected.is_empty());
        self.stalls += 1;
    }
    fn finish(mut self, disconnect: bool) {
        for _ in 0..3 {
            self.pump();
        }
        if disconnect {
            for _ in 0..4 {
                self.control
                    .as_mut()
                    .unwrap()
                    .submit(
                        Lane::Normal,
                        Command::resource(Request::Info {
                            resource: self.playback,
                        }),
                    )
                    .map_err(|e| e.error)
                    .unwrap();
            }
            drop(self.control.take());
            for _ in 0..32 {
                self.pump();
            }
            return;
        }
        if self.capture.is_some() {
            self.toggle_capture();
        }
        for index in 0..4 {
            if let Some(lease) = self.leases[index].take() {
                Self::reply(&self.resource(
                    Request::ReleaseSnapshot {
                        snapshot: lease.snapshot.id,
                    },
                    Lane::Recovery,
                ));
            }
        }
        let plan = self.plan(false, true);
        let epoch = self.runtime.epoch();
        self.sign = 0.;
        let rendered = self.runtime.next_frame().0;
        assert_eq!(
            self.command(Command::commit(epoch, plan), Lane::Normal)
                .outcome,
            Outcome::Applied
        );
        let removed = self.command(Command::Remove { node: node_id() }, Lane::Normal);
        let Some(Retired::Processor(node)) = removed.retired else {
            panic!("processor retirement")
        };
        assert_eq!(node.frames, rendered);
        Self::reply(&self.resource(
            Request::ReleaseSnapshot {
                snapshot: node.snapshot.id,
            },
            Lane::Recovery,
        ));
        drop(node);
        for resource in [self.playback, self.recording] {
            Self::reply(&self.resource(Request::Remove { resource }, Lane::Normal));
        }
        for _ in 0..16 {
            self.pump();
        }
        let usage = self.runtime.resource_usage();
        assert_eq!(usage.free_pages, 128);
        assert_eq!(usage.reserved_pages, 0);
        assert_eq!(usage.free_versions, 16);
        assert_eq!(usage.free_snapshots, 16);
        assert_eq!(usage.retired_pages, 0);
        assert_eq!(usage.retired_versions, 0);
        assert_eq!(self.control.as_ref().unwrap().outstanding(), 0);
    }
}

#[shoop_test]
fn seeded_mixed_graph_timing_capture_revision_stalls_disconnects_and_restarts_are_bounded() {
    let seeds = if cfg!(miri) { 1 } else { 8 };
    let rounds = if cfg!(miri) { 8 } else { 1024 };
    let mut callbacks = 0;
    let mut replacements = 0;
    let mut snapshots = 0;
    let mut stalls = 0;
    let mut peak_pages = 0;
    let mut peak_owners = 0;
    for seed in 1..=seeds {
        let drops = Arc::new(AtomicUsize::new(0));
        let mut host = Host::new(Arc::clone(&drops));
        let mut random = Random(0x864e_92b7 ^ seed);
        for _ in 0..rounds {
            let value = random.next();
            match value % 8 {
                0 => host.rewire(value & 16 != 0, value & 32 != 0),
                1 => host.schedule(value, false, value % 11 == 0),
                2 => host.schedule(value, true, value % 13 == 0),
                3 => host.replace([0.25, 0.5, 1., 2.][(value >> 8) as usize % 4]),
                4 => host.toggle_capture(),
                5 => host.snapshot((value >> 8) as usize % 4),
                6 => host.read((value >> 8) as usize % 4),
                _ => host.stall(),
            }
        }
        callbacks += host.callbacks;
        replacements += host.replacements;
        snapshots += host.snapshots;
        stalls += host.stalls;
        peak_pages = peak_pages.max(host.peak_pages);
        peak_owners = peak_owners.max(host.peak_owners);
        assert!(host.peak_owners <= 5);
        host.finish(seed % 2 == 0);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }
    println!("mixed stress: callbacks={callbacks} replacements={replacements} snapshots={snapshots} stalls={stalls} peak_pages={peak_pages} peak_owners={peak_owners}");
    if !cfg!(miri) {
        assert!(callbacks > 20_000 && replacements > 500 && snapshots > 500 && stalls > 500);
        assert!(peak_pages > 32 && peak_pages <= 128);
    }
}
