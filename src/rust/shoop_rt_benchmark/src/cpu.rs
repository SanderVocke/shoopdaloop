use crate::{
    fixture::*,
    runner::{Control, Driver, Measurement},
};
use anyhow::{anyhow, ensure, Result};
use rt_runtime::buffer::{AudioBlock, MidiBlock, MidiBuffer};
use shoop_rt_engine::Render;
use shoop_rt_engine::{EngineDescription, HostPorts, Segment};
use shoop_rt_protocol::*;
use std::time::Instant;
const SHARED_TAKE_PAGES: u32 = 65536;

pub struct Cpu {
    render: Render,
    inputs: Vec<Vec<f32>>,
    outputs: Vec<Vec<f32>>,
    midi_inputs: Vec<MidiBuffer>,
    midi_outputs: Vec<MidiBuffer>,
    packets: Vec<MidiPacket>,
    emitted: Vec<MidiPacket>,
    media: Media,
    pool_pages: usize,
    latency_frames: usize,
    input_origin: u64,
}
impl Driver for Cpu {
    fn begin_fixture(&mut self) {
        self.input_origin = self.render.next_frame().0;
    }
    fn next_frame(&self) -> u64 {
        self.render.next_frame().0
    }
    fn fill_input(&mut self) {
        let first = self.render.next_frame().0 - self.input_origin;
        for (channel, samples) in self.inputs.iter_mut().enumerate() {
            for (offset, sample) in samples.iter_mut().enumerate() {
                *sample = input(channel, first + offset as u64);
            }
        }
        self.packets.clear();
        if self.media.midi() {
            midi_input(first, &mut self.packets);
            self.midi_inputs[0].clear();
            for packet in &self.packets {
                self.midi_inputs[0]
                    .push(packet.offset, QUANTUM, &packet.bytes)
                    .expect("prepared MIDI input capacity");
            }
        }
    }
    fn process(&mut self) -> Result<Measurement> {
        for output in &mut self.midi_outputs {
            output.clear();
        }
        let inputs: arrayvec::ArrayVec<&[f32], 2> = self.inputs.iter().map(Vec::as_slice).collect();
        let mut outputs: arrayvec::ArrayVec<&mut [f32], 2> =
            self.outputs.iter_mut().map(Vec::as_mut_slice).collect();
        let start = Instant::now();
        let result = self
            .render
            .process(
                AudioBlock {
                    frame: self.render.next_frame(),
                    frames: QUANTUM,
                    inputs: &inputs,
                    outputs: &mut outputs,
                },
                MidiBlock {
                    inputs: &self.midi_inputs,
                    outputs: &mut self.midi_outputs,
                },
            )
            .map_err(|e| anyhow!("CPU render failed: {e:?}"))?;
        let elapsed_ns = start.elapsed().as_nanos() as u64;
        ensure!(
            result.phase == shoop_rt_engine::Phase::Running,
            "CPU render stopped: {:?}",
            result.phase
        );
        let usage = self.render.resource_usage();
        let work = self.render.resource_work().0;
        self.emitted.clear();
        if self.media.midi() {
            for index in 0..self.midi_outputs[0].len() {
                let (offset, bytes) = self.midi_outputs[0].get(index).unwrap();
                ensure!(bytes.len() == 3, "unexpected MIDI packet size");
                self.emitted.push(MidiPacket {
                    offset,
                    bytes: bytes.try_into()?,
                });
            }
            ensure!(
                self.render.midi_recovery(0).is_none_or(|b| b.is_empty()),
                "unexpected MIDI recovery"
            );
        }
        Ok(Measurement {
            elapsed_ns,
            storage_bytes: (self.pool_pages.saturating_sub(usage.free_pages) * 1024) as u64,
            reclaimed: work.reclaimed_pages as u64,
            midi_input_events: self.packets.len() as u64,
            midi_output_events: self.emitted.len() as u64,
            ..Default::default()
        })
    }
    fn output(&self, channel: usize) -> &[f32] {
        &self.outputs[channel]
    }
    fn latency_frames(&self) -> usize {
        self.latency_frames
    }
    fn supports_grab(&self) -> bool {
        !self.media.midi()
    }
    fn midi_latency_frames(&self) -> usize {
        self.latency_frames
    }
    fn midi_output(&self) -> &[MidiPacket] {
        &self.emitted
    }
    fn metadata(&self) -> serde_json::Value {
        serde_json::json!({"driver":"stepped-dummy","numeric":"f32","timing":"host-wall-clock","processing_latency_frames":self.latency_frames,"page_bytes":1024,"resource_pages":MEMORY_BYTES/1024,"shared_take_capacity_bytes":SHARED_TAKE_PAGES as usize*1024,"smoothing_frames":0,"ramp_frames":0})
    }
}

pub fn prepare(fixture: &Fixture) -> Result<(Control<shoop_rt_engine::ControlEndpoint>, Cpu)> {
    let media = fixture.media;
    let channels = media.audio_channels();
    let d = EngineDescription {
        quantum: QUANTUM,
        max_frames: QUANTUM,
        sample_rate: SAMPLE_RATE,
        host: HostPorts::numbered(channels, channels, usize::from(media.midi())),
        segments: vec![Segment {
            strips: 1,
            loops: LOOPS,
        }],
        resource_pages: MEMORY_BYTES / 1024,
        page_bytes: 1024,
        smoothing_frames: 0,
        smoothing_limit_frames: 0,
        ramp_frames: 0,
        control_lead_frames: 0,
        pending_transitions: 1024,
        reliable_credits: 256,
        bulk_credits: 1,
        midi_events: if media.midi() { MIDI_INPUT_CAPACITY } else { 1 },
        midi_bytes: if media.midi() {
            MIDI_INPUT_CAPACITY * 3
        } else {
            16
        },
        master_connections: (0..channels).map(|i| (i as u8, i as u16)).collect(),
        history: host::InputHistorySettings {
            seconds: if media.midi() {
                0
            } else {
                fixture.history_seconds
            },
            audio_take_pages: SHARED_TAKE_PAGES,
            midi_pages: 16,
            midi_records: 16,
            writers: 128,
        },
        ..Default::default()
    };
    let prepared = shoop_rt_engine::prepare(
        &d,
        DriverStatus {
            kind: DriverKind::Dummy,
            state: DriverRunState::Running,
            sample_rate: SAMPLE_RATE,
            buffer_frames: QUANTUM as u32,
        },
    )
    .map_err(|e| anyhow!("CPU preparation: {e:?}"))?;
    let outbox = prepared.outbox();
    let render = prepared.render;
    let endpoint = prepared.endpoint;
    let mut control = Control::new(endpoint, outbox)?;
    let pool_pages = render.resource_usage().free_pages;
    let latency_frames = control.endpoint.limits().processing_latency_frames as usize;
    let mut cpu = Cpu {
        render,
        inputs: vec![vec![0.; QUANTUM]; channels],
        outputs: vec![vec![0.; QUANTUM]; channels],
        midi_inputs: (0..usize::from(media.midi()))
            .map(|_| MidiBuffer::new(MIDI_INPUT_CAPACITY, MIDI_INPUT_CAPACITY * 3).unwrap())
            .collect(),
        midi_outputs: (0..usize::from(media.midi()))
            .map(|_| {
                MidiBuffer::new(MIDI_OUTPUT_CAPACITY + 64, (MIDI_OUTPUT_CAPACITY + 64) * 3).unwrap()
            })
            .collect(),
        packets: Vec::with_capacity(MIDI_INPUT_CAPACITY),
        emitted: Vec::with_capacity(MIDI_OUTPUT_CAPACITY + 64),
        media,
        pool_pages,
        latency_frames,
        input_origin: 0,
    };
    let strip = StripId(SlotId::new(0, 1));
    let mut requests = vec![Request::CreateStrip {
        strip,
        shape: StripShape {
            audio_inputs: channels as u8,
            audio_outputs: channels as u8,
            midi_input: media.midi(),
            midi_output: media.midi(),
        },
    }];
    for index in 0..channels as u8 {
        requests.push(Request::Connect {
            port: EnginePort::Strip(StripPort {
                strip,
                kind: PortKind::AudioIn,
                index,
            }),
            host: HostPortId(index as u32),
            connected: true,
        });
        requests.push(Request::Route {
            port: StripPort {
                strip,
                kind: PortKind::AudioOut,
                index,
            },
            bus: BusPort {
                bus: BusId::MASTER,
                index,
            },
            connected: true,
        });
    }
    if media.midi() {
        for (kind, host) in [
            (PortKind::MidiIn, channels * 2),
            (PortKind::MidiOut, channels * 2 + 1),
        ] {
            requests.push(Request::Connect {
                port: EnginePort::Strip(StripPort {
                    strip,
                    kind,
                    index: 0,
                }),
                host: HostPortId(host as u32),
                connected: true,
            });
        }
    }
    for index in 0..LOOPS {
        requests.push(Request::CreateLoop {
            loop_id: LoopSlotId(SlotId::new(index, 1)),
            strip,
        });
    }
    for index in 1..LOOPS {
        requests.push(Request::LoopSync {
            loop_id: LoopSlotId(SlotId::new(index, 1)),
            source: Some(LoopSlotId(SlotId::new(0, 1))),
        });
    }
    for (i, request) in requests.into_iter().enumerate() {
        let correlation = i as u64 + 1;
        control.send(ClientMessage::Request {
            correlation,
            request,
        })?;
        let mut done = false;
        for _ in 0..256 {
            control.pump()?;
            cpu.fill_input();
            cpu.process()?;
            control.pump()?;
            if let Some((_, outcome)) = control.results.iter().find(|(id, _)| *id == correlation) {
                ensure!(
                    *outcome == Outcome::Done,
                    "CPU setup refused: {request:?}: {outcome:?}"
                );
                done = true;
                break;
            }
        }
        ensure!(done, "CPU setup did not complete: {request:?}");
    }
    control.snapshot()?;
    let commands: Vec<_> = (0..LOOPS)
        .map(|id| immediate::Command::LoopGain(id, GAIN))
        .collect();
    for group in commands.chunks(immediate::COMMANDS) {
        control.send(ClientMessage::Immediate(
            immediate::Batch::new(control.configuration.0, control.configuration.1, group).unwrap(),
        ))?;
    }
    control.responses.clear();
    for _ in 0..256 {
        control.pump()?;
        cpu.fill_input();
        cpu.process()?;
        control.pump()?;
        if control
            .responses
            .iter()
            .map(|r| r.records().len())
            .sum::<usize>()
            == LOOPS as usize
            && control.endpoint.is_idle()
        {
            break;
        }
    }
    ensure!(
        control
            .responses
            .iter()
            .map(|r| r.records().len())
            .sum::<usize>()
            == LOOPS as usize,
        "initial loop gains did not settle"
    );
    control.snapshot()?;
    Ok((control, cpu))
}
