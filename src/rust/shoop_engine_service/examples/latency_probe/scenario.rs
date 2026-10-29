use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    midi_state::MidiState,
    resource::{Cursor, Reply as ReadReply, Request as ReadRequest},
    transport::Outcome,
    Frame,
};
use shoop_engine_service::{
    dsp, CaptureTap, Client, Config, EngineCommand, ExecutionStatus, LoopOperation as Op,
    LoopReply as Reply, Render, RequestId,
};
use shoop_latency::{
    wet_recording_offset, CaptureFrameMapping, ProcessorRenderAdvance, RecordingOffset,
    RetentionWindow,
};
use shoop_loop_engine::{runtime::Selection, Mode, Timing, Transition};
use shoop_wasm_test_support::heap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Signal {
    Ramp,
    Impulses,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Capture {
    Raw,
    Tail,
    Margins,
}
impl Capture {
    pub fn interval(self) -> std::ops::Range<u64> {
        match self {
            Self::Raw => 4..12,
            Self::Tail => 4..15,
            Self::Margins => 2..13,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub enum Partition {
    Whole,
    Single,
    Irregular,
}
pub fn dry(signal: Signal, frame: u64) -> f32 {
    match signal {
        Signal::Ramp => (frame + 1) as f32,
        Signal::Impulses => match frame {
            2 => -1.,
            4 => 1.,
            11 => 0.5,
            12 => 0.25,
            _ => 0.,
        },
    }
}
pub fn wet(signal: Signal, frame: u64) -> f32 {
    frame.checked_sub(3).map_or(0., |at| dry(signal, at))
}
const MARKERS: [(u64, [u8; 3]); 5] = [
    (2, [0xB0, 74, 2]),
    (4, [0xB0, 74, 4]),
    (11, [0xB0, 74, 11]),
    (12, [0xB0, 74, 12]),
    (14, [0xB0, 74, 14]),
];
#[derive(Debug, PartialEq)]
pub struct Evidence {
    pub signal: Signal,
    pub capture: Capture,
    /// Unshifted DSP output at every absolute engine frame, including the tail.
    pub physical_wet: Vec<f32>,
    /// Unmodified interleaved dry/wet resource samples, relative to capture start.
    pub raw_capture: Vec<[f32; 2]>,
    pub output_markers: Vec<(u64, Vec<u8>)>,
    pub raw_midi: Vec<u8>,
}
impl Evidence {
    /// A proposed window is a separate observation, never replacement media.
    pub fn window(&self, channel: usize, start: u64, end: u64) -> Vec<Option<f32>> {
        let retained = self.capture.interval();
        (start..end)
            .map(|frame| {
                retained
                    .contains(&frame)
                    .then(|| self.raw_capture[(frame - retained.start) as usize][channel])
            })
            .collect()
    }
    pub fn verify_windows(&self) {
        for (offset, dry_start, wet_start, absent) in [
            (
                0,
                4,
                7,
                match self.capture {
                    Capture::Raw => [0, 3],
                    Capture::Tail => [0, 0],
                    Capture::Margins => [0, 2],
                },
            ),
            (
                -2,
                2,
                5,
                match self.capture {
                    Capture::Raw => [2, 1],
                    Capture::Tail => [2, 0],
                    Capture::Margins => [0, 0],
                },
            ),
        ] {
            let dry_offset = RecordingOffset::new(offset).unwrap();
            let wet_offset =
                wet_recording_offset(dry_offset, ProcessorRenderAdvance::new(3).unwrap()).unwrap();
            assert_eq!(
                CaptureFrameMapping::new(dry_offset.frames())
                    .unwrap()
                    .raw_frame(4)
                    .unwrap(),
                dry_start as i64
            );
            assert_eq!(
                CaptureFrameMapping::new(wet_offset.frames())
                    .unwrap()
                    .raw_frame(4)
                    .unwrap(),
                wet_start as i64
            );
            assert_eq!(
                CaptureFrameMapping::processor_dispatch_frame(wet_start as i64, 3).unwrap(),
                dry_start as i64
            );
            let retained = RetentionWindow::for_offset(dry_offset);
            assert_eq!(
                (retained.before_frames(), retained.after_frames()),
                if offset == 0 { (0, 0) } else { (2, 0) }
            );
            let retained = RetentionWindow::for_offset(wet_offset);
            assert_eq!(
                (retained.before_frames(), retained.after_frames()),
                if offset == 0 { (0, 3) } else { (0, 1) }
            );
            for (channel, start) in [dry_start, wet_start].into_iter().enumerate() {
                let observed = self.window(channel, start, start + 8);
                assert_eq!(
                    observed.iter().filter(|x| x.is_none()).count(),
                    absent[channel]
                );
                for (i, value) in observed.into_iter().enumerate() {
                    if let Some(value) = value {
                        let logical_input = (4 + offset + i as i32) as u64;
                        assert_eq!(value, dry(self.signal, logical_input));
                    }
                }
            }
        }
    }
}
struct Host {
    client: Client,
    render: Render,
    target: shoop_loop_engine::LoopId,
    input: [MidiBuffer; 1],
    output: [MidiBuffer; 1],
    signal: Signal,
    replies: Vec<(RequestId, Reply)>,
    reads: Vec<(RequestId, Vec<u8>)>,
    physical_wet: Vec<f32>,
    output_markers: Vec<(u64, Vec<u8>)>,
    pattern: usize,
}
impl Host {
    fn ask(&mut self, at: u64, operation: Op) -> RequestId {
        heap::assert_no_heap(|| self.client.loop_at(Frame(at), operation).unwrap().id())
    }
    fn pump(&mut self) {
        while let Some(delivery) = self.client.try_delivery() {
            assert!(!matches!(
                delivery.outcome,
                Outcome::Rejected(_) | Outcome::Cancelled
            ));
            if let Some(EngineCommand::Resource(command)) = delivery.command {
                let Some(ReadReply::Read { written, eof }) = command.reply() else {
                    panic!("read receipt");
                };
                assert!(eof);
                let ReadRequest::Read { cursor, bytes } = command.into_request() else {
                    panic!("owning buffer");
                };
                assert_eq!(cursor.position(), written);
                self.reads.push((delivery.id, bytes[..written].to_vec()));
            }
        }
        while let Some(execution) = self.client.try_execution() {
            let ExecutionStatus::Executed(raw) = execution.status else {
                panic!("actual execution");
            };
            assert_eq!(raw.result, Ok(()));
            assert_eq!(raw.late_by(), 0);
            let reply = raw.operation_result.unwrap();
            if let Reply::Transition(completion) = reply {
                assert!(matches!(
                    completion.outcome,
                    shoop_loop_engine::Outcome::Applied { .. }
                ));
            } else {
                self.replies.push((execution.id.unwrap(), reply));
            }
        }
    }
    fn advance(&mut self, end: u64, partition: Partition) {
        while self.render.next_frame().0 < end {
            let frame = self.render.next_frame().0;
            let remaining = (end - frame) as usize;
            let n = match partition {
                Partition::Whole => remaining,
                Partition::Single => 1,
                Partition::Irregular => {
                    let n = [0, 3, 1, 5, 0, 2][self.pattern % 6];
                    self.pattern += 1;
                    n.min(remaining)
                }
            };
            let mut left = [0.; 64];
            let mut right = [0.; 64];
            heap::assert_no_heap(|| {
                self.input[0].clear();
                for (at, bytes) in MARKERS {
                    if frame <= at && at < frame + n as u64 {
                        self.input[0].push((at - frame) as u32, n, &bytes).unwrap();
                    }
                }
                let input: [f32; 64] = std::array::from_fn(|i| dry(self.signal, frame + i as u64));
                let report = self
                    .render
                    .process(
                        AudioBlock {
                            frame: Frame(frame),
                            frames: n,
                            inputs: &[&input[..n], &input[..n]],
                            outputs: &mut [&mut left[..n], &mut right[..n]],
                        },
                        MidiBlock {
                            inputs: &self.input,
                            outputs: &mut self.output,
                        },
                    )
                    .unwrap();
                assert_eq!(report.next_frame, Frame(frame + n as u64));
                if n == 0 {
                    assert_eq!(report.work, shoop_engine_service::Work::default());
                }
                assert!(self.render.midi_recovery().is_empty());
            });
            for i in 0..n {
                let expected = wet(self.signal, frame + i as u64);
                assert_eq!(left[i], expected);
                assert_eq!(right[i], expected);
            }
            self.physical_wet.extend_from_slice(&right[..n]);
            for i in 0..self.output[0].len() {
                let (at, bytes) = self.output[0].get(i).unwrap();
                self.output_markers
                    .push((frame + at as u64, bytes.to_vec()));
            }
            self.pump();
        }
    }
    fn reply(&mut self, id: RequestId) -> Reply {
        let at = self.replies.iter().position(|(key, _)| *key == id).unwrap();
        self.replies.remove(at).1
    }
    fn cursor(&mut self, id: RequestId) -> Cursor {
        let Reply::Cursor(cursor) = self.reply(id) else {
            panic!("cursor");
        };
        cursor
    }
    fn read(&mut self, cursor: Cursor) -> RequestId {
        let bytes = vec![0; 256].into_boxed_slice();
        heap::assert_no_heap(|| self.client.read(cursor, bytes).unwrap().id())
    }
    fn bytes(&mut self, id: RequestId) -> Vec<u8> {
        let index = self.reads.iter().position(|(key, _)| *key == id).unwrap();
        self.reads.remove(index).1
    }
}
pub fn run(signal: Signal, capture: Capture, partition: Partition) -> Evidence {
    let preset = dsp::PresetId::from_parts(0, 1).unwrap();
    let prepared = shoop_engine_service::prepare(
        Config {
            capture: CaptureTap::DryWet,
            resource_pages: 128,
            ..Config::default()
        },
        vec![dsp::Preset {
            id: preset,
            live_gain: [1.; 2],
            playback_gain: [0.; 2],
            swap_channels: false,
            delay_frames: 3,
        }],
        preset,
    )
    .unwrap();
    let mut host = Host {
        client: prepared.client,
        render: prepared.render,
        target: prepared.target,
        input: [MidiBuffer::new(32, 128).unwrap()],
        output: [MidiBuffer::new(128, 512).unwrap()],
        signal,
        replies: Vec::with_capacity(16),
        reads: Vec::with_capacity(2),
        physical_wet: Vec::with_capacity(32),
        output_markers: Vec::with_capacity(16),
        pattern: 0,
    };
    let interval = capture.interval();
    host.ask(
        interval.start,
        Op::Transition {
            target: host.target,
            timing: Timing::At,
            transition: Transition::Record {
                monitor_previous: false,
            },
        },
    );
    host.ask(
        interval.end,
        Op::Transition {
            target: host.target,
            timing: Timing::At,
            transition: Transition::Stop,
        },
    );
    let exported = host.ask(
        interval.end,
        Op::Export {
            target: host.target,
            selection: Selection::Latest,
        },
    );
    let query = host.ask(
        interval.end,
        Op::State {
            target: host.target,
        },
    );
    host.advance(16, partition);
    let Reply::State(cut) = host.reply(query) else {
        panic!("settled state");
    };
    assert_eq!(cut.frame, Frame(interval.end));
    assert_eq!(cut.state.mode, Mode::Stopped);
    assert_eq!(cut.state.content_length, interval.end - interval.start);
    let Reply::Export { id, media, cut } = host.reply(exported) else {
        panic!("export");
    };
    assert_eq!(cut.frame, Frame(interval.end));
    assert_eq!(media.view.description.frames, interval.end - interval.start);
    let audio = host.ask(
        16,
        Op::Cursor {
            export: id,
            midi: false,
        },
    );
    let midi = host.ask(
        16,
        Op::Cursor {
            export: id,
            midi: true,
        },
    );
    let initial = host.ask(
        16,
        Op::MidiInitial {
            export: id,
            channel: 0,
        },
    );
    host.advance(17, partition);
    let Reply::MidiInitial {
        state,
        system_reset_seen,
        ..
    } = host.reply(initial)
    else {
        panic!("initial");
    };
    let mut expected = MidiState::default();
    for (at, bytes) in MARKERS {
        if at < interval.start {
            expected.apply_packet(&bytes, 512).unwrap();
        }
    }
    assert!(!system_reset_seen);
    assert_eq!(state, *expected.channel(0).unwrap());
    let cursor = host.cursor(audio);
    let audio = host.read(cursor);
    let cursor = host.cursor(midi);
    let midi = host.read(cursor);
    host.advance(18, partition);
    let audio = host.bytes(audio);
    let raw_midi = host.bytes(midi);
    let expected_audio: Vec<u8> = interval
        .clone()
        .flat_map(|at| [dry(signal, at), wet(signal, at)])
        .flat_map(f32::to_le_bytes)
        .collect();
    assert_eq!(audio, expected_audio);
    let mut expected_midi = Vec::new();
    for (at, bytes) in MARKERS {
        if interval.contains(&at) {
            expected_midi.extend_from_slice(&(at - interval.start).to_le_bytes());
            expected_midi.extend_from_slice(&3u32.to_le_bytes());
            expected_midi.extend_from_slice(&bytes);
        }
    }
    assert_eq!(raw_midi, expected_midi);
    let raw_capture = audio
        .chunks_exact(8)
        .map(|bytes| {
            [
                f32::from_le_bytes(bytes[..4].try_into().unwrap()),
                f32::from_le_bytes(bytes[4..].try_into().unwrap()),
            ]
        })
        .collect();
    let released = host.ask(18, Op::Release { export: id });
    host.advance(19, partition);
    assert_eq!(host.reply(released), Reply::Done);
    assert_eq!(
        host.output_markers,
        MARKERS.map(|(at, bytes)| (at, bytes.to_vec()))
    );
    assert!(host.replies.is_empty());
    assert!(host.reads.is_empty());
    assert_eq!(host.client.outstanding(), 0);
    assert_eq!(host.render.diagnostics().midi_overflows, 0);
    let evidence = Evidence {
        signal,
        capture,
        physical_wet: host.physical_wet,
        raw_capture,
        output_markers: host.output_markers,
        raw_midi,
    };
    assert_eq!(evidence.physical_wet.len(), 19);
    evidence.verify_windows();
    drop(host.client);
    let mut stopped = host.render.into_stopped();
    stopped.cleanup().unwrap();
    assert_eq!(stopped.resource_usage().free_pages, 128);
    assert_eq!(stopped.resource_usage().free_versions, 32);
    assert_eq!(stopped.resource_usage().free_snapshots, 32);
    assert_eq!(stopped.midi_index_usage().free_versions, 8);
    let usage = stopped.media_usage();
    assert_eq!(
        (
            usage.free_media,
            usage.free_exports,
            usage.free_pins,
            usage.references
        ),
        (8, 4, 8, 0)
    );
    evidence
}
