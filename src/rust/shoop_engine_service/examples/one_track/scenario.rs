use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    midi_state::{ChannelState, MidiState},
    resource::{Cursor, Reply as ReadReply, Request as ReadRequest},
    transport::Outcome,
    Error, Frame,
};
use shoop_engine_service::{
    dsp, prepare_with_content, Client, Config, EngineCommand, ExecutionStatus, InitialContent,
    InitialMidi, LoopOperation as Op, LoopReply as Reply, Render, RequestId,
};
use shoop_loop_engine::{
    media::{ExportId, MediaId},
    runtime::{Failure, Selection},
    Mode, Timing, Transition,
};
use shoop_wasm_test_support::heap;

#[derive(Clone, Copy, Debug)]
pub enum Partition {
    Whole,
    Single,
    Irregular,
}
#[derive(Debug)]
pub struct Summary {
    pub frames: u64,
    pub packets: usize,
    pub copied_bytes: usize,
    pub callbacks: usize,
}
type Packet = (u64, Vec<u8>);
struct Read {
    id: RequestId,
    cursor: Cursor,
    bytes: Vec<u8>,
    eof: bool,
}
struct Host {
    client: Client,
    render: Render,
    target: shoop_loop_engine::LoopId,
    input: [MidiBuffer; 1],
    output: [MidiBuffer; 1],
    events: Vec<Packet>,
    left: Vec<f32>,
    right: Vec<f32>,
    packets: Vec<Packet>,
    replies: Vec<(RequestId, Reply)>,
    reads: Vec<Read>,
    stage: Option<rt_runtime::graph::activation::StageId>,
    rejected_read: Option<RequestId>,
    rejection_seen: bool,
    peaks: [f32; 2],
    callbacks: usize,
    pattern: usize,
    dsp_applied: bool,
}
fn preset(slot: u32) -> dsp::PresetId {
    dsp::PresetId::from_parts(slot, 1).unwrap()
}
fn initial() -> InitialContent {
    let mut state = MidiState::default();
    state.apply_packet(&[0x90, 62, 40], 3).unwrap();
    InitialContent {
        audio: [vec![-10., -20., -30., -40.], vec![100., 200., 300., 400.]],
        midi_initial: state,
        midi: vec![
            InitialMidi {
                frame: 0,
                bytes: vec![0xB0, 7, 77],
            },
            InitialMidi {
                frame: 1,
                bytes: vec![0x80, 62, 20],
            },
            InitialMidi {
                frame: 2,
                bytes: vec![0x90, 64, 70],
            },
            InitialMidi {
                frame: 3,
                bytes: vec![0x80, 64, 30],
            },
        ],
    }
}
fn live() -> Vec<Packet> {
    vec![
        (0, vec![0x90, 60, 100]),
        (2, vec![0x80, 60, 45]),
        (3, vec![0x90, 61, 80]),
        (4, vec![0x80, 61, 33]),
        (12, vec![0xB0, 10, 35]),
        (23, vec![0x90, 65, 90]),
        (24, vec![0x80, 65, 40]),
        (25, vec![0xB0, 11, 99]),
    ]
}
impl Host {
    fn new() -> Self {
        let bank = vec![
            dsp::Preset {
                id: preset(0),
                live_gain: [1.; 2],
                playback_gain: [1.; 2],
                swap_channels: false,
                delay_frames: 0,
            },
            dsp::Preset {
                id: preset(1),
                live_gain: [2., -1.],
                playback_gain: [0.5, 0.25],
                swap_channels: true,
                delay_frames: 0,
            },
        ];
        let prepared = prepare_with_content(
            Config {
                resource_pages: 512,
                ..Config::default()
            },
            bank,
            preset(0),
            initial(),
        )
        .unwrap();
        let mut host = Self {
            client: prepared.client,
            render: prepared.render,
            target: prepared.target,
            input: [MidiBuffer::new(2048, 8192).unwrap()],
            output: [MidiBuffer::new(2048, 8192).unwrap()],
            events: live(),
            left: Vec::with_capacity(64),
            right: Vec::with_capacity(64),
            packets: Vec::with_capacity(1024),
            replies: Vec::with_capacity(32),
            reads: Vec::with_capacity(8),
            stage: None,
            rejected_read: None,
            rejection_seen: false,
            peaks: [0.; 2],
            callbacks: 0,
            pattern: 0,
            dsp_applied: false,
        };
        host.action(
            0,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        );
        host.ask(
            0,
            Op::Export {
                target: host.target,
                selection: Selection::Playback,
            },
        );
        host.action(
            1,
            Transition::Record {
                monitor_previous: false,
            },
        );
        host.ask(
            4,
            Op::Export {
                target: host.target,
                selection: Selection::Recording,
            },
        );
        host.client
            .stage_route(prepared.route, rt_runtime::Epoch(1))
            .unwrap();
        host
    }
    fn ask(&mut self, at: u64, op: Op) -> RequestId {
        heap::assert_no_heap(|| self.client.loop_at(Frame(at), op).unwrap().id())
    }
    fn action(&mut self, at: u64, transition: Transition) -> RequestId {
        self.ask(
            at,
            Op::Transition {
                target: self.target,
                timing: Timing::At,
                transition,
            },
        )
    }
    fn pump(&mut self) {
        while let Some(delivery) = self.client.try_delivery() {
            if self.rejected_read == Some(delivery.id) {
                assert_eq!(delivery.outcome, Outcome::Rejected(Error::InvalidId));
                assert!(matches!(delivery.command, Some(EngineCommand::Resource(_))));
                self.rejection_seen = true;
                continue;
            }
            assert!(
                !matches!(delivery.outcome, Outcome::Rejected(_) | Outcome::Cancelled),
                "{:?}",
                delivery.outcome
            );
            match delivery.command {
                Some(EngineCommand::StageGraph { id, .. }) => self.stage = id,
                Some(EngineCommand::Resource(command)) => {
                    let Some(ReadReply::Read { written, eof }) = command.reply() else {
                        panic!("read reply");
                    };
                    let ReadRequest::Read { cursor, bytes } = command.into_request() else {
                        panic!("owning buffer");
                    };
                    self.reads.push(Read {
                        id: delivery.id,
                        cursor,
                        bytes: bytes[..written].to_vec(),
                        eof,
                    });
                }
                _ => {}
            }
        }
        while let Some(execution) = self.client.try_execution() {
            let ExecutionStatus::Executed(raw) = execution.status else {
                panic!("actual loop execution");
            };
            assert_eq!(raw.result, Ok(()));
            assert_eq!(raw.late_by(), 0);
            let reply = raw.operation_result.unwrap();
            if let Reply::Transition(completion) = reply {
                assert!(
                    matches!(
                        completion.outcome,
                        shoop_loop_engine::Outcome::Applied { .. }
                    ),
                    "{completion:?}"
                );
            } else {
                self.replies.push((execution.id.unwrap(), reply));
            }
        }
        while let Some(completion) = self.client.dsp().try_completion() {
            assert_eq!(
                completion.request.result,
                Some(Ok(dsp::Receipt::Applied {
                    requested_frame: Frame(5),
                    frame: Frame(5),
                    previous_revision: 1,
                    revision: 2,
                    preset: preset(1)
                }))
            );
            assert!(!self.dsp_applied);
            self.dsp_applied = true;
        }
    }
    fn step(&mut self, frames: usize) {
        let start = self.render.next_frame().0;
        let mut left = [0.; 64];
        let mut right = [0.; 64];
        let report = heap::assert_no_heap(|| {
            self.input[0].clear();
            for (at, bytes) in &self.events {
                if start <= *at && *at < start + frames as u64 {
                    self.input[0]
                        .push((*at - start) as u32, frames, bytes)
                        .unwrap();
                }
            }
            let input: [[f32; 64]; 2] = std::array::from_fn(|channel| {
                std::array::from_fn(|i| {
                    (start + i as u64 + 1) as f32 * if channel == 0 { 1. } else { 10. }
                })
            });
            self.render.process(
                AudioBlock {
                    frame: Frame(start),
                    frames,
                    inputs: &[&input[0][..frames], &input[1][..frames]],
                    outputs: &mut [&mut left[..frames], &mut right[..frames]],
                },
                MidiBlock {
                    inputs: &self.input,
                    outputs: &mut self.output,
                },
            )
        })
        .unwrap();
        assert_eq!(report.next_frame, Frame(start + frames as u64));
        assert!(self.render.midi_recovery().is_empty());
        if frames == 0 {
            assert_eq!(report.work, shoop_engine_service::Work::default());
        }
        self.callbacks += 1;
        self.left.extend_from_slice(&left[..frames]);
        self.right.extend_from_slice(&right[..frames]);
        for i in 0..self.output[0].len() {
            let (at, bytes) = self.output[0].get(i).unwrap();
            self.packets.push((start + at as u64, bytes.to_vec()));
        }
        for _ in 0..3 {
            let read = heap::assert_no_heap(|| self.client.meters().read_peaks());
            for (peak, value) in self.peaks.iter_mut().zip(read) {
                *peak = peak.max(value);
            }
        }
        self.pump();
    }
    fn advance(&mut self, end: u64, mode: Partition) {
        while self.render.next_frame().0 < end {
            let remaining = (end - self.render.next_frame().0) as usize;
            let frames = match mode {
                Partition::Whole => remaining.min(64),
                Partition::Single => 1,
                Partition::Irregular => {
                    let n = [0, 3, 1, 5, 0, 2, 4][self.pattern % 7];
                    self.pattern += 1;
                    n.min(remaining)
                }
            };
            self.step(frames);
        }
    }
    fn reply(&mut self, id: RequestId) -> Reply {
        let index = self
            .replies
            .iter()
            .position(|(request, _)| *request == id)
            .expect("reply ID");
        self.replies.remove(index).1
    }
    fn export(&mut self, frame: u64, frames: u64) -> (ExportId, MediaId) {
        let index = self
            .replies
            .iter()
            .position(|(_, reply)| {
                matches!(reply, Reply::Export { cut, media, .. }
            if cut.frame == Frame(frame) && media.view.description.frames == frames)
            })
            .expect("export cut");
        let Reply::Export { id, media, .. } = self.replies.remove(index).1 else {
            unreachable!();
        };
        (id, media.media)
    }
    fn cursor(&mut self, id: RequestId) -> Cursor {
        let Reply::Cursor(cursor) = self.reply(id) else {
            panic!("cursor");
        };
        cursor
    }
    fn read(&mut self, cursor: Cursor) -> RequestId {
        let buffer = vec![0; 256].into_boxed_slice();
        heap::assert_no_heap(|| self.client.read(cursor, buffer).unwrap().id())
    }
    fn bytes(&mut self, id: RequestId) -> Vec<u8> {
        let index = self.reads.iter().position(|read| read.id == id).unwrap();
        let read = self.reads.remove(index);
        assert!(read.eof);
        assert_eq!(read.cursor.position(), read.bytes.len());
        read.bytes
    }
    fn metadata(&mut self, ids: &[(u8, RequestId)], channels: &mut [ChannelState; 16]) {
        for &(channel, id) in ids {
            let Reply::MidiInitial {
                channel: actual,
                state,
                system_reset_seen,
            } = self.reply(id)
            else {
                panic!("initial metadata");
            };
            assert_eq!(actual, channel);
            assert!(!system_reset_seen);
            channels[channel as usize] = state;
        }
    }
    fn state(
        &mut self,
        id: RequestId,
        frame: u64,
        mode: Mode,
        media: Option<MediaId>,
    ) -> shoop_loop_engine::runtime::Cut {
        let Reply::State(cut) = self.reply(id) else {
            panic!("state");
        };
        assert_eq!(cut.frame, Frame(frame));
        assert_eq!(cut.graph_epoch.0, 2);
        assert_eq!(cut.topology_epoch.0, 0);
        assert_eq!(cut.state.mode, mode);
        assert_eq!(cut.capture_failure, None);
        assert_eq!(cut.midi_overflows, 0);
        if let Some(media) = media {
            assert_eq!(cut.playback, Some(media));
        }
        cut
    }
}
fn restore(output: &mut Vec<Packet>, at: u64, control: bool, notes: &[(u8, u8)]) {
    for channel in 0..16u8 {
        if channel == 0 && control {
            output.push((at, vec![0xB0, 7, 77]));
        }
        for bytes in [
            [0xE0 | channel, 0, 64],
            [0xB0 | channel, 64, 0],
            [0xB0 | channel, 69, 0],
            [0xB0 | channel, 66, 0],
        ] {
            output.push((at, bytes.to_vec()));
        }
        if channel == 0 {
            for &(key, velocity) in notes {
                output.push((at, vec![0x90, key, velocity]));
            }
        }
    }
}
fn golden_packets() -> Vec<Packet> {
    let mut p = vec![(0, vec![0x90, 60, 100])];
    restore(&mut p, 0, false, &[(62, 40)]);
    p.push((0, vec![0xB0, 7, 77]));
    p.extend([
        (1, vec![0x80, 62, 0]),
        (2, vec![0x80, 60, 45]),
        (3, vec![0x90, 61, 80]),
        (4, vec![0x80, 61, 33]),
        (12, vec![0xB0, 10, 35]),
    ]);
    restore(&mut p, 12, true, &[(60, 100), (62, 40)]);
    p.extend([
        (12, vec![0x80, 62, 0]),
        (13, vec![0x80, 60, 45]),
        (14, vec![0x90, 61, 80]),
        (15, vec![0x80, 61, 33]),
        (23, vec![0x90, 65, 90]),
    ]);
    restore(&mut p, 23, true, &[(60, 100), (62, 40)]);
    p.extend([
        (23, vec![0x80, 62, 0]),
        (24, vec![0x80, 65, 40]),
        (24, vec![0x80, 60, 45]),
        (25, vec![0xB0, 11, 99]),
    ]);
    restore(&mut p, 25, true, &[(61, 80)]);
    p.push((25, vec![0x80, 61, 33]));
    restore(&mut p, 33, true, &[(60, 100), (62, 40)]);
    p.extend([(33, vec![0x80, 62, 0]), (34, vec![0x80, 60, 45])]);
    restore(&mut p, 35, false, &[(62, 40)]);
    p.extend([
        (35, vec![0xB0, 7, 77]),
        (36, vec![0x80, 62, 20]),
        (37, vec![0x90, 64, 70]),
        (38, vec![0x80, 64, 30]),
    ]);
    restore(&mut p, 39, false, &[(62, 40)]);
    p.extend([(39, vec![0xB0, 7, 77]), (40, vec![0x80, 62, 0])]);
    p
}
fn golden_audio() -> (Vec<f32>, Vec<f32>) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    for frame in 0..42 {
        let live = (frame + 1) as f32;
        let playback = match frame {
            0 => [-10., 100.],
            12..=22 => {
                let value = (frame - 10) as f32;
                [value, 10. * value]
            }
            23..=24 => {
                let value = (frame - 21) as f32;
                [value, 10. * value]
            }
            25..=32 => {
                let value = (frame - 20) as f32;
                [value, 10. * value]
            }
            33..=34 => {
                let value = (frame - 31) as f32;
                [value, 10. * value]
            }
            35..=39 => {
                let index = (frame - 35) % 4;
                [(-10 * (index + 1)) as f32, (100 * (index + 1)) as f32]
            }
            _ => [0.; 2],
        };
        if frame < 5 {
            left.push(live + playback[0]);
            right.push(10. * live + playback[1]);
        } else {
            left.push(20. * live + 0.5 * playback[1]);
            right.push(-live + 0.25 * playback[0]);
        }
    }
    (left, right)
}
fn audio_bytes(frames: std::ops::Range<u64>) -> Vec<u8> {
    frames
        .flat_map(|i| [i as f32, 10. * i as f32])
        .flat_map(f32::to_le_bytes)
        .collect()
}
fn midi_bytes(packets: &[(u64, [u8; 3])]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for (at, packet) in packets {
        bytes.extend_from_slice(&at.to_le_bytes());
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(packet);
    }
    bytes
}

pub fn verify_stopped_input() {
    let bank = vec![dsp::Preset {
        id: preset(0),
        live_gain: [1.; 2],
        playback_gain: [1.; 2],
        swap_channels: false,
        delay_frames: 0,
    }];
    let prepared = shoop_engine_service::prepare(
        Config {
            resource_pages: 128,
            ..Config::default()
        },
        bank,
        preset(0),
    )
    .unwrap();
    let mut client = prepared.client;
    let mut render = prepared.render;
    let target = prepared.target;
    client.loop_at(Frame(0), Op::State { target }).unwrap();
    for (at, transition) in [
        (
            1,
            Transition::Record {
                monitor_previous: false,
            },
        ),
        (
            4,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        ),
    ] {
        client
            .loop_at(
                Frame(at),
                Op::Transition {
                    target,
                    timing: Timing::At,
                    transition,
                },
            )
            .unwrap();
    }
    client
        .loop_at(
            Frame(4),
            Op::Export {
                target,
                selection: Selection::Playback,
            },
        )
        .unwrap();
    let mut input = [MidiBuffer::new(2, 6).unwrap()];
    let mut output = [MidiBuffer::new(512, 2048).unwrap()];
    input[0].push(0, 8, &[0x90, 60, 100]).unwrap();
    input[0].push(2, 8, &[0x80, 60, 45]).unwrap();
    let mut left = [0.; 8];
    let mut right = [0.; 8];
    heap::assert_no_heap(|| {
        render.process(
            AudioBlock {
                frame: Frame(0),
                frames: 8,
                inputs: &[
                    &[1., 2., 3., 4., 5., 6., 7., 8.],
                    &[10., 20., 30., 40., 50., 60., 70., 80.],
                ],
                outputs: &mut [&mut left, &mut right],
            },
            MidiBlock {
                inputs: &input,
                outputs: &mut output,
            },
        )
    })
    .unwrap();
    assert_eq!(left, [1., 2., 3., 4., 7., 9., 11., 10.]);
    assert_eq!(right, left.map(|sample| sample * 10.));
    let mut expected = vec![(0, vec![0x90, 60, 100]), (2, vec![0x80, 60, 45])];
    restore(&mut expected, 4, false, &[(60, 100)]);
    expected.push((5, vec![0x80, 60, 45]));
    restore(&mut expected, 7, false, &[(60, 100)]);
    let actual = (0..output[0].len())
        .map(|i| {
            let (at, p) = output[0].get(i).unwrap();
            (at as u64, p.to_vec())
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
    assert!(render.midi_recovery().is_empty());
    while client.try_delivery().is_some() {}
    let mut export = None;
    let mut stopped_seen = false;
    while let Some(execution) = client.try_execution() {
        let ExecutionStatus::Executed(raw) = execution.status else {
            panic!("actual outcome");
        };
        assert_eq!(raw.result, Ok(()));
        match raw.operation_result {
            Some(Reply::State(cut)) => {
                assert_eq!(cut.frame, Frame(0));
                assert_eq!(cut.state.mode, Mode::Stopped);
                stopped_seen = true;
            }
            Some(Reply::Export { id, .. }) => export = Some(id),
            Some(Reply::Transition(c)) => assert!(matches!(
                c.outcome,
                shoop_loop_engine::Outcome::Applied { .. }
            )),
            reply => panic!("unexpected stopped-input result: {reply:?}"),
        }
    }
    assert!(stopped_seen);
    client
        .loop_at(
            Frame(8),
            Op::MidiInitial {
                export: export.unwrap(),
                channel: 0,
            },
        )
        .unwrap();
    client.stop().unwrap();
    input[0].clear();
    let mut out_left = [0.];
    let mut out_right = [0.];
    heap::assert_no_heap(|| {
        render.process(
            AudioBlock {
                frame: Frame(8),
                frames: 1,
                inputs: &[&[0.], &[0.]],
                outputs: &mut [&mut out_left, &mut out_right],
            },
            MidiBlock {
                inputs: &input,
                outputs: &mut output,
            },
        )
    })
    .unwrap();
    assert_eq!(output[0].len(), 1);
    assert_eq!(output[0].get(0).unwrap(), (0, &[0x80, 60, 0][..]));
    while client.try_delivery().is_some() {}
    let mut initial_seen = false;
    while let Some(execution) = client.try_execution() {
        let ExecutionStatus::Executed(raw) = execution.status else {
            panic!("initial state outcome");
        };
        if let Some(Reply::MidiInitial {
            state,
            system_reset_seen,
            ..
        }) = raw.operation_result
        {
            assert!(!system_reset_seen);
            assert!(state.note(60).unwrap().held());
            assert_eq!(state.note(60).unwrap().velocity(), 100);
            initial_seen = true;
        }
    }
    assert!(initial_seen);
    drop(client);
    let mut stopped = render.into_stopped();
    stopped.cleanup().unwrap();
    assert_eq!(stopped.resource_usage().free_pages, 128);
    assert_eq!(stopped.resource_usage().free_versions, 32);
    assert_eq!(stopped.resource_usage().free_snapshots, 32);
    assert_eq!(stopped.midi_index_usage().free_versions, 8);
    assert_eq!(stopped.media_usage().references, 0);
}

pub fn run(mode: Partition, alternate: bool, inject: Option<&str>) -> Summary {
    let mut h = Host::new();
    h.advance(2, mode);
    let (original_export, original) = h.export(0, 4);
    let stage = h.stage.expect("graph Ready before future cut");
    let retired = h.ask(2, Op::Retire { media: original });
    let query4 = if alternate {
        None
    } else {
        Some(h.ask(4, Op::State { target: h.target }))
    };
    let activated = h.ask(
        4,
        Op::GraphActivate {
            id: stage,
            graph_epoch: rt_runtime::Epoch(1),
            topology_epoch: rt_runtime::Epoch(0),
        },
    );
    let query4 = query4.unwrap_or_else(|| h.ask(4, Op::State { target: h.target }));
    h.client.dsp().select(Frame(5), preset(1), 1).unwrap();
    h.advance(5, mode);
    assert_eq!(h.reply(retired), Reply::Done);
    assert!(matches!(
        h.reply(activated),
        Reply::GraphActivated {
            graph_epoch: rt_runtime::Epoch(2),
            topology_epoch: rt_runtime::Epoch(0),
            ..
        }
    ));
    let cut = h.state(query4, 4, Mode::Recording, None);
    assert_eq!(cut.state.capture_frames, 3);
    let (prefix, _) = h.export(4, 3);
    let cursor_audio = h.ask(
        5,
        Op::Cursor {
            export: prefix,
            midi: false,
        },
    );
    let cursor_midi = h.ask(
        5,
        Op::Cursor {
            export: prefix,
            midi: true,
        },
    );
    let meta0 = (0..6u8)
        .map(|channel| {
            (
                channel,
                h.ask(
                    5,
                    Op::MidiInitial {
                        export: prefix,
                        channel,
                    },
                ),
            )
        })
        .collect::<Vec<_>>();
    h.advance(6, mode);
    let mut channels = [ChannelState::default(); 16];
    h.metadata(&meta0, &mut channels);
    let audio_cursor = h.cursor(cursor_audio);
    let midi_cursor = h.cursor(cursor_midi);
    let audio_read = h.read(audio_cursor);
    let midi_read = h.read(midi_cursor);
    let meta1 = (6..12u8)
        .map(|channel| {
            (
                channel,
                h.ask(
                    6,
                    Op::MidiInitial {
                        export: prefix,
                        channel,
                    },
                ),
            )
        })
        .collect::<Vec<_>>();
    let query6 = h.ask(6, Op::State { target: h.target });
    h.advance(7, mode);
    h.state(query6, 6, Mode::Recording, None);
    h.metadata(&meta1, &mut channels);
    let prefix_audio = h.bytes(audio_read);
    let prefix_midi = h.bytes(midi_read);
    assert_eq!(prefix_audio, audio_bytes(2..5));
    assert_eq!(
        prefix_midi,
        midi_bytes(&[(0, [0x80, 62, 0]), (1, [0x80, 60, 45]), (2, [0x90, 61, 80])])
    );
    let meta2 = (12..16u8)
        .map(|channel| {
            (
                channel,
                h.ask(
                    7,
                    Op::MidiInitial {
                        export: prefix,
                        channel,
                    },
                ),
            )
        })
        .collect::<Vec<_>>();
    let cursor_audio = h.ask(
        7,
        Op::Cursor {
            export: prefix,
            midi: false,
        },
    );
    let cursor_midi = h.ask(
        7,
        Op::Cursor {
            export: prefix,
            midi: true,
        },
    );
    let initial_audio = h.ask(
        7,
        Op::Cursor {
            export: original_export,
            midi: false,
        },
    );
    let initial_midi = h.ask(
        7,
        Op::Cursor {
            export: original_export,
            midi: true,
        },
    );
    h.advance(8, mode);
    h.metadata(&meta2, &mut channels);
    let mut expected_state = MidiState::default();
    for (_, packet) in golden_packets().into_iter().take_while(|(at, _)| *at < 1) {
        expected_state.apply_packet(&packet, 512).unwrap();
    }
    let actual_state = MidiState::from_channels(channels, false);
    assert_eq!(actual_state, expected_state);
    assert!(channels[0].note(60).unwrap().held());
    assert_eq!(channels[0].note(60).unwrap().velocity(), 100);
    assert!(channels[0].note(62).unwrap().held());
    assert_eq!(channels[0].note(62).unwrap().velocity(), 40);
    let cursor = h.cursor(cursor_audio);
    let again_audio = h.read(cursor);
    let cursor = h.cursor(cursor_midi);
    let again_midi = h.read(cursor);
    let cursor = h.cursor(initial_audio);
    let initial_audio = h.read(cursor);
    let cursor = h.cursor(initial_midi);
    let initial_midi = h.read(cursor);
    h.advance(9, mode);
    assert_eq!(h.bytes(again_audio), prefix_audio);
    assert_eq!(h.bytes(again_midi), prefix_midi);
    let initial = initial();
    let expected_initial_audio = (0..4)
        .flat_map(|i| [initial.audio[0][i], initial.audio[1][i]])
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    assert_eq!(h.bytes(initial_audio), expected_initial_audio);
    assert_eq!(
        h.bytes(initial_midi),
        midi_bytes(&[
            (0, [0xB0, 7, 77]),
            (1, [0x80, 62, 20]),
            (2, [0x90, 64, 70]),
            (3, [0x80, 64, 30])
        ])
    );
    let release = h.ask(9, Op::Release { export: prefix });
    let double = h.ask(9, Op::Release { export: prefix });
    h.action(
        12,
        Transition::Play {
            position: 0,
            retrigger: false,
        },
    );
    let completed = h.ask(
        12,
        Op::Export {
            target: h.target,
            selection: Selection::Playback,
        },
    );
    let query12 = h.ask(12, Op::State { target: h.target });
    let initial_state = h.ask(
        9,
        Op::MidiInitial {
            export: original_export,
            channel: 0,
        },
    );
    h.advance(10, mode);
    assert_eq!(h.reply(release), Reply::Done);
    assert_eq!(
        h.reply(double),
        Reply::Failed(Failure::Runtime(Error::InvalidId))
    );
    let Reply::MidiInitial {
        state,
        system_reset_seen,
        ..
    } = h.reply(initial_state)
    else {
        panic!("original metadata");
    };
    assert!(!system_reset_seen);
    assert_eq!(state, *initial.midi_initial.channel(0).unwrap());
    let buffer = vec![0; 256].into_boxed_slice();
    h.rejected_read = Some(h.client.read(audio_cursor, buffer).unwrap().id());
    let query10 = h.ask(10, Op::State { target: h.target });
    h.advance(17, mode);
    assert!(h.rejection_seen);
    h.state(query10, 10, Mode::Recording, None);
    let Reply::Export {
        id: completed_export,
        media,
        cut,
    } = h.reply(completed)
    else {
        panic!("completed media");
    };
    assert_eq!(cut.frame, Frame(12));
    assert_eq!(media.view.description.frames, 11);
    assert_eq!(media.view.description.revision, rt_runtime::Revision(1));
    let recorded = media.media;
    h.state(query12, 12, Mode::Playing, Some(recorded));
    let query25 = if alternate {
        None
    } else {
        Some(h.ask(25, Op::State { target: h.target }))
    };
    h.action(
        25,
        Transition::Play {
            position: 3,
            retrigger: true,
        },
    );
    let query25 = query25.unwrap_or_else(|| h.ask(25, Op::State { target: h.target }));
    let query35 = if alternate {
        None
    } else {
        Some(h.ask(35, Op::State { target: h.target }))
    };
    h.ask(
        35,
        Op::Select {
            target: h.target,
            timing: Timing::At,
            media: original,
            preserve_position: false,
        },
    );
    let query35 = query35.unwrap_or_else(|| h.ask(35, Op::State { target: h.target }));
    let query40 = if alternate {
        None
    } else {
        Some(h.ask(40, Op::State { target: h.target }))
    };
    h.action(40, Transition::Stop);
    let query40 = query40.unwrap_or_else(|| h.ask(40, Op::State { target: h.target }));
    let audio = h.ask(
        17,
        Op::Cursor {
            export: completed_export,
            midi: false,
        },
    );
    let midi = h.ask(
        17,
        Op::Cursor {
            export: completed_export,
            midi: true,
        },
    );
    h.advance(26, mode);
    let cut = h.state(query25, 25, Mode::Playing, Some(recorded));
    assert_eq!(cut.state.position, 3);
    let cursor = h.cursor(audio);
    let audio = h.read(cursor);
    let cursor = h.cursor(midi);
    let midi = h.read(cursor);
    let completed_meta = h.ask(
        26,
        Op::MidiInitial {
            export: completed_export,
            channel: 0,
        },
    );
    h.advance(35, mode);
    let complete_audio = h.bytes(audio);
    let complete_midi = h.bytes(midi);
    assert_eq!(complete_audio, audio_bytes(2..13));
    assert_eq!(
        complete_midi,
        midi_bytes(&[
            (0, [0x80, 62, 0]),
            (1, [0x80, 60, 45]),
            (2, [0x90, 61, 80]),
            (3, [0x80, 61, 33])
        ])
    );
    let Reply::MidiInitial {
        state,
        system_reset_seen,
        ..
    } = h.reply(completed_meta)
    else {
        panic!("completed initial state");
    };
    assert!(!system_reset_seen);
    assert_eq!(state, channels[0]);
    let release_original = h.ask(
        41,
        Op::Release {
            export: original_export,
        },
    );
    let release_completed = h.ask(
        41,
        Op::Release {
            export: completed_export,
        },
    );
    h.client.release_route(stage).unwrap();
    h.advance(42, mode);
    let cut = h.state(query35, 35, Mode::Playing, Some(original));
    assert_eq!(cut.state.content_revision, rt_runtime::Revision(0));
    assert_eq!(cut.state.content_length, 4);
    assert_eq!(
        cut.graph_epoch.0,
        if inject == Some("outcome") { 99 } else { 2 }
    );
    h.state(query40, 40, Mode::Stopped, Some(original));
    assert_eq!(h.reply(release_original), Reply::Done);
    assert_eq!(h.reply(release_completed), Reply::Done);
    assert!(h.replies.is_empty());
    assert!(h.reads.is_empty());
    assert_eq!(h.client.outstanding(), 0);
    assert!(h.dsp_applied);
    let (mut left, right) = golden_audio();
    let mut packets = golden_packets();
    if inject == Some("sample") {
        left[0] += 1.;
    }
    if inject == Some("packet") {
        packets[0].1[2] ^= 1;
    }
    assert_eq!(h.left, left);
    assert_eq!(h.right, right);
    assert_eq!(h.packets, packets);
    assert_eq!(h.peaks, [980., 110.]);
    assert_eq!(h.render.diagnostics().midi_overflows, 0);
    let summary = Summary {
        frames: h.render.next_frame().0,
        packets: h.packets.len(),
        copied_bytes: prefix_audio.len() * 2
            + prefix_midi.len() * 2
            + expected_initial_audio.len()
            + 60
            + complete_audio.len()
            + complete_midi.len(),
        callbacks: h.callbacks,
    };
    drop(h.client);
    let mut stopped = h.render.into_stopped();
    stopped.cleanup().unwrap();
    stopped.cleanup().unwrap();
    let usage = stopped.resource_usage();
    assert_eq!(usage.free_pages, 512);
    assert_eq!(usage.free_versions, 32);
    assert_eq!(usage.free_snapshots, 32);
    assert_eq!(usage.reserved_pages, 0);
    assert_eq!(usage.retired_pages, 0);
    assert_eq!(usage.retired_versions, 0);
    assert_eq!(stopped.midi_index_usage().free_versions, 8);
    let usage = stopped.media_usage();
    assert_eq!(usage.free_media, 8);
    assert_eq!(usage.free_exports, 4);
    assert_eq!(usage.free_pins, 8);
    assert_eq!(usage.references, 0);
    summary
}
