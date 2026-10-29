use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    midi_state::MidiState,
    resource::{Cursor, Reply as ReadReply, Request as ReadRequest},
    transport::Outcome,
};
use shoop_engine_service::{
    dsp, media::ExportId, Client, Config, EngineCommand, ExecutionStatus, InitialContent,
    LoopOperation as Op, LoopReply as Reply, Render,
};
use shoop_loop_engine::{runtime::Selection, Mode as LoopMode, Timing, Transition};
use shoop_wasm_test_support::heap;
use std::{hint::black_box, time::Instant};

pub const FRAMES: usize = 64;
pub const LENGTH: usize = 4096;
const PAGES: usize = 8192;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Idle,
    Steady,
    Preset,
    Poll,
    Export,
}
impl Mode {
    fn parse(value: &str) -> Self {
        match value {
            "idle" => Self::Idle,
            "steady" => Self::Steady,
            "preset" => Self::Preset,
            "poll" => Self::Poll,
            "export" => Self::Export,
            _ => panic!("unknown workload"),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Steady => "steady",
            Self::Preset => "preset",
            Self::Poll => "poll",
            Self::Export => "export",
        }
    }
    fn playing(self) -> bool {
        matches!(self, Self::Steady | Self::Preset | Self::Poll)
    }
}
#[derive(Clone, Copy, Default)]
pub struct Counters {
    pub midi: usize,
    pub copied: usize,
    pub forwarded: usize,
    pub returned: usize,
    pub executed: usize,
}
#[derive(Clone, Copy, Default)]
pub struct Admission {
    pub attempts: usize,
    pub refused: usize,
}
#[derive(Clone, Copy, Default)]
struct Row {
    frame: u64,
    callback_ns: u64,
    control_ns: u64,
    meter_ns: u64,
    attempts: usize,
    refused: usize,
    meter_reads: usize,
    work: Counters,
}
pub trait Bench {
    fn frame(&self) -> u64;
    fn control(&mut self) -> Admission;
    fn process(&mut self) -> Counters;
    fn collect(&mut self, work: &mut Counters);
    fn meters(&mut self);
    fn check(&self, frame: u64);
    fn finish(self);
}
fn nanos(start: Instant) -> u64 {
    start.elapsed().as_nanos().try_into().unwrap()
}
pub fn rss_kib() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find(|line| line.starts_with("VmRSS:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}
fn measure<B: Bench>(mut bench: B, engine: &str, mode: Mode, samples: usize, warmup: usize) {
    let mut rows = vec![
        Row {
            frame: u64::MAX,
            ..Row::default()
        };
        samples
    ];
    let mut clocks = [0u64; 256];
    for clock in &mut clocks {
        let start = Instant::now();
        *clock = nanos(start);
    }
    clocks.sort_unstable();
    eprintln!("meta engine={engine} mode={} frames={FRAMES} loop_frames={LENGTH} sample_rate=48000 samples={samples} warmup={warmup} debug={} clock_median_ns={} rss_constructed_kib={:?}", mode.name(), cfg!(debug_assertions), clocks[128], rss_kib());
    for _ in 0..warmup {
        let frame = bench.frame();
        bench.control();
        let mut work = heap::assert_no_heap(|| bench.process());
        bench.collect(&mut work);
        bench.check(frame);
        bench.meters();
    }
    eprintln!("meta rss_after_warmup_kib={:?}", rss_kib());
    let wall = Instant::now();
    for row in &mut rows {
        row.frame = bench.frame();
        let start = Instant::now();
        let admission = bench.control();
        row.control_ns = nanos(start);
        row.attempts = admission.attempts;
        row.refused = admission.refused;
        let start = Instant::now();
        let work = heap::assert_no_heap(|| bench.process());
        let callback_ns = nanos(start);
        row.work = work;
        row.callback_ns = callback_ns;
        bench.collect(&mut row.work);
        bench.check(row.frame);
        row.meter_reads = if mode == Mode::Poll { 64 } else { 1 };
        let start = Instant::now();
        for _ in 0..row.meter_reads {
            bench.meters();
        }
        row.meter_ns = nanos(start);
    }
    let wall_ns = nanos(wall);
    eprintln!(
        "meta elapsed_wall_ns={wall_ns} rss_live_kib={:?}",
        rss_kib()
    );
    bench.finish();
    eprintln!("meta rss_after_cleanup_kib={:?}", rss_kib());
    println!("engine,workload,index,frame,callback_ns,control_ns,meter_ns,control_attempts,refusals,meter_reads,midi_packets,copied_bytes,forwarded,owning_returns,executions");
    for (i, row) in rows.into_iter().enumerate() {
        println!(
            "{engine},{},{i},{},{},{},{},{},{},{},{},{},{},{},{}",
            mode.name(),
            row.frame,
            row.callback_ns,
            row.control_ns,
            row.meter_ns,
            row.attempts,
            row.refused,
            row.meter_reads,
            row.work.midi,
            row.work.copied,
            row.work.forwarded,
            row.work.returned,
            row.work.executed
        );
    }
}
pub fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(
        args.len(),
        4,
        "usage: measure_service service|current idle|steady|preset|poll|export SAMPLES WARMUP"
    );
    let mode = Mode::parse(&args[1]);
    let samples: usize = args[2].parse().unwrap();
    let warmup: usize = args[3].parse().unwrap();
    assert!((16..=2048).contains(&samples));
    assert!((1..=512).contains(&warmup));
    match args[0].as_str() {
        "service" => measure(Service::new(mode), "service", mode, samples, warmup),
        "current" => {
            assert!(matches!(mode, Mode::Idle | Mode::Steady));
            measure(
                super::current::Current::new(mode),
                "current",
                mode,
                samples,
                warmup,
            );
        }
        _ => panic!("unknown engine"),
    }
}
fn preset(slot: u32) -> dsp::PresetId {
    dsp::PresetId::from_parts(slot, 1).unwrap()
}
#[derive(Clone, Copy)]
enum Bulk {
    NeedExport,
    WaitExport,
    NeedCursor,
    WaitCursor,
    NeedRead(Cursor),
    WaitRead,
    NeedRelease,
    WaitRelease,
}
struct Service {
    client: Client,
    render: Render,
    target: shoop_loop_engine::LoopId,
    mode: Mode,
    input: [MidiBuffer; 1],
    output: [MidiBuffer; 1],
    left: [f32; FRAMES],
    right: [f32; FRAMES],
    revision: u32,
    bulk: Bulk,
    export: Option<ExportId>,
    read_buffer: Option<Box<[u8]>>,
}
impl Service {
    fn new(mode: Mode) -> Self {
        let config = Config {
            resource_pages: PAGES,
            ..Config::default()
        };
        let presets: Vec<_> = (0..2)
            .map(|slot| dsp::Preset {
                id: preset(slot),
                live_gain: [1.; 2],
                playback_gain: [1.; 2],
                swap_channels: false,
                delay_frames: 0,
            })
            .collect();
        let prepared = if mode == Mode::Export {
            shoop_engine_service::prepare(config, presets, preset(0))
        } else {
            shoop_engine_service::prepare_with_content(
                config,
                presets,
                preset(0),
                InitialContent {
                    audio: [vec![0.5; LENGTH], vec![1.; LENGTH]],
                    midi: Vec::new(),
                    midi_initial: MidiState::default(),
                },
            )
        }
        .unwrap();
        let mut host = Self {
            client: prepared.client,
            render: prepared.render,
            target: prepared.target,
            mode,
            input: [MidiBuffer::new(2048, 8192).unwrap()],
            output: [MidiBuffer::new(2048, 8192).unwrap()],
            left: [0.; FRAMES],
            right: [0.; FRAMES],
            revision: 1,
            bulk: Bulk::NeedExport,
            export: None,
            read_buffer: Some(vec![0; 256].into_boxed_slice()),
        };
        if mode.playing() || mode == Mode::Export {
            let transition = if mode == Mode::Export {
                Transition::Record {
                    monitor_previous: false,
                }
            } else {
                Transition::Play {
                    position: 0,
                    retrigger: false,
                }
            };
            host.ask(Op::Transition {
                target: host.target,
                timing: Timing::At,
                transition,
            });
        }
        host
    }
    fn ask(&mut self, operation: Op) {
        heap::assert_no_heap(|| {
            self.client
                .loop_at(self.render.next_frame(), operation)
                .unwrap()
        });
    }
}
impl Bench for Service {
    fn frame(&self) -> u64 {
        self.render.next_frame().0
    }
    fn control(&mut self) -> Admission {
        let mut admission = Admission::default();
        match self.mode {
            Mode::Preset => {
                let at = self.render.next_frame();
                heap::assert_no_heap(|| {
                    self.client
                        .dsp()
                        .select(at, preset((at.0 / FRAMES as u64 % 2) as u32), self.revision)
                        .unwrap()
                });
                let error =
                    heap::assert_no_heap(|| self.client.dsp().select(at, preset(0), self.revision))
                        .unwrap_err();
                assert_eq!(error.error, rt_runtime::Error::Full);
                admission.attempts = 2;
                admission.refused = 1;
            }
            Mode::Poll => {
                self.ask(Op::State {
                    target: self.target,
                });
                admission.attempts = 1;
            }
            Mode::Export if self.frame() >= 128 => {
                match self.bulk {
                    Bulk::NeedExport => {
                        self.ask(Op::Export {
                            target: self.target,
                            selection: Selection::Recording,
                        });
                        self.bulk = Bulk::WaitExport;
                    }
                    Bulk::NeedCursor => {
                        self.ask(Op::Cursor {
                            export: self.export.unwrap(),
                            midi: false,
                        });
                        self.bulk = Bulk::WaitCursor;
                    }
                    Bulk::NeedRead(cursor) => {
                        let buffer = self.read_buffer.take().unwrap();
                        heap::assert_no_heap(|| self.client.read(cursor, buffer).unwrap());
                        self.bulk = Bulk::WaitRead;
                    }
                    Bulk::NeedRelease => {
                        self.ask(Op::Release {
                            export: self.export.unwrap(),
                        });
                        self.bulk = Bulk::WaitRelease;
                    }
                    _ => return admission,
                }
                admission.attempts = 1;
            }
            _ => {}
        }
        admission
    }
    fn process(&mut self) -> Counters {
        let report = self
            .render
            .process(
                AudioBlock {
                    frame: self.render.next_frame(),
                    frames: FRAMES,
                    inputs: &[&[0.25; FRAMES], &[0.5; FRAMES]],
                    outputs: &mut [&mut self.left, &mut self.right],
                },
                MidiBlock {
                    inputs: &self.input,
                    outputs: &mut self.output,
                },
            )
            .unwrap();
        assert!(self.render.midi_recovery().is_empty());
        Counters {
            midi: self.output[0].len(),
            forwarded: report.work.forwarded,
            returned: report.work.owning_returns,
            executed: report.work.executions,
            copied: 0,
        }
    }
    fn collect(&mut self, work: &mut Counters) {
        while let Some(delivery) = self.client.try_delivery() {
            assert!(!matches!(
                delivery.outcome,
                Outcome::Rejected(_) | Outcome::Cancelled
            ));
            if let Some(EngineCommand::Resource(command)) = delivery.command {
                assert!(matches!(self.bulk, Bulk::WaitRead));
                let Some(ReadReply::Read { written, eof }) = command.reply() else {
                    panic!("read receipt");
                };
                let ReadRequest::Read { cursor, bytes } = command.into_request() else {
                    panic!("read owner");
                };
                let start = cursor.position() - written;
                let expected = [0, 0, 128, 62, 0, 0, 0, 63];
                for i in 0..written {
                    assert_eq!(bytes[i], expected[(start + i) % 8]);
                }
                work.copied += written;
                assert!(self.read_buffer.replace(bytes).is_none());
                self.bulk = if eof {
                    Bulk::NeedRelease
                } else {
                    Bulk::NeedRead(cursor)
                };
            }
        }
        while let Some(execution) = self.client.try_execution() {
            let ExecutionStatus::Executed(raw) = execution.status else {
                panic!("actual execution");
            };
            assert_eq!(raw.result, Ok(()));
            assert_eq!(raw.late_by(), 0);
            match raw.operation_result.unwrap() {
                Reply::Transition(completion) => assert!(matches!(
                    completion.outcome,
                    shoop_loop_engine::Outcome::Applied { .. }
                )),
                Reply::State(cut) => {
                    assert_eq!(cut.state.mode, LoopMode::Playing);
                    assert_eq!(cut.capture_failure, None);
                }
                Reply::Export { id, media, cut } => {
                    assert!(matches!(self.bulk, Bulk::WaitExport));
                    assert_eq!(media.view.description.frames, cut.frame.0);
                    self.export = Some(id);
                    self.bulk = Bulk::NeedCursor;
                }
                Reply::Cursor(cursor) => {
                    assert!(matches!(self.bulk, Bulk::WaitCursor));
                    self.bulk = Bulk::NeedRead(cursor);
                }
                Reply::Done => {
                    assert!(matches!(self.bulk, Bulk::WaitRelease));
                    self.export = None;
                    self.bulk = Bulk::NeedExport;
                }
                reply => panic!("unexpected {reply:?}"),
            }
        }
        while let Some(completion) = self.client.dsp().try_completion() {
            let Some(Ok(dsp::Receipt::Applied {
                previous_revision,
                revision,
                frame,
                requested_frame,
                ..
            })) = completion.request.result
            else {
                panic!("DSP receipt");
            };
            assert_eq!(previous_revision, self.revision);
            assert_eq!(revision, self.revision + 1);
            assert_eq!(frame, requested_frame);
            self.revision = revision;
        }
        assert_eq!(self.client.outstanding(), 0);
    }
    fn meters(&mut self) {
        black_box(heap::assert_no_heap(|| self.client.meters().read_peaks()));
    }
    fn check(&self, frame: u64) {
        let expected = if self.mode.playing() {
            [0.75, 1.5]
        } else {
            [0.25, 0.5]
        };
        assert_eq!(self.left, [expected[0]; FRAMES]);
        assert_eq!(self.right, [expected[1]; FRAMES]);
        let restore = self.mode.playing() && frame % LENGTH as u64 == 0;
        assert_eq!(self.output[0].len(), if restore { 64 } else { 0 });
        if restore {
            for channel in 0..16u8 {
                for (i, packet) in [
                    [0xE0 | channel, 0, 64],
                    [0xB0 | channel, 64, 0],
                    [0xB0 | channel, 69, 0],
                    [0xB0 | channel, 66, 0],
                ]
                .iter()
                .enumerate()
                {
                    assert_eq!(
                        self.output[0].get(channel as usize * 4 + i).unwrap(),
                        (0, packet.as_slice())
                    );
                }
            }
        }
    }
    fn finish(mut self) {
        if let Some(export) = self.export {
            self.ask(Op::Release { export });
            self.bulk = Bulk::WaitRelease;
        }
        self.client.stop().unwrap();
        let mut work = heap::assert_no_heap(|| self.process());
        self.collect(&mut work);
        drop(self.client);
        let mut stopped = self.render.into_stopped();
        eprintln!("meta service_resource_pages={PAGES} pool_before_cleanup={:?} media_before_cleanup={:?}", stopped.resource_usage(), stopped.media_usage());
        stopped.cleanup().unwrap();
        let pool = stopped.resource_usage();
        assert_eq!(pool.free_pages, PAGES);
        assert_eq!(pool.free_versions, 32);
        assert_eq!(pool.free_snapshots, 32);
        assert_eq!(pool.reserved_pages, 0);
        assert_eq!(pool.retired_pages, 0);
        assert_eq!(pool.retired_versions, 0);
        assert_eq!(stopped.midi_index_usage().free_versions, 8);
        let media = stopped.media_usage();
        assert_eq!(
            (
                media.free_media,
                media.free_exports,
                media.free_pins,
                media.references
            ),
            (8, 4, 8, 0)
        );
        eprintln!("meta pool_after_cleanup={pool:?}");
    }
}
