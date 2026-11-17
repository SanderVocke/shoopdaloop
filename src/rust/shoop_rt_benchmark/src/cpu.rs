use crate::{
    fixture::*,
    runner::{Control, Driver, Measurement},
};
use anyhow::{anyhow, ensure, Result};
use shoop_rt_engine::{EngineDescription, HostPorts, Segment};
use shoop_rt_host_native::SteppedDummy;
use shoop_rt_protocol::*;
use std::time::Instant;
const SHARED_TAKE_PAGES: u32 = 65536;

pub struct Cpu {
    driver: SteppedDummy,
    pool_pages: usize,
    latency_frames: usize,
    input_origin: u64,
}
impl Driver for Cpu {
    fn begin_fixture(&mut self) {
        self.input_origin = self.driver.next_frame();
    }
    fn next_frame(&self) -> u64 {
        self.driver.next_frame()
    }
    fn fill_input(&mut self) {
        let first = self.driver.next_frame() - self.input_origin;
        for (channel, samples) in self.driver.inputs_mut().iter_mut().enumerate() {
            for (offset, sample) in samples.iter_mut().enumerate() {
                *sample = input(channel, first + offset as u64);
            }
        }
    }
    fn process(&mut self) -> Result<Measurement> {
        let start = Instant::now();
        self.driver
            .process()
            .map_err(|e| anyhow!("CPU render failed: {e:?}"))?;
        let elapsed_ns = start.elapsed().as_nanos() as u64;
        let usage = self.driver.resource_usage();
        let work = self.driver.resource_work();
        Ok(Measurement {
            elapsed_ns,
            storage_bytes: (self.pool_pages.saturating_sub(usage.free_pages) * 1024) as u64,
            reclaimed: work.reclaimed_pages as u64,
            ..Default::default()
        })
    }
    fn output(&self, channel: usize) -> &[f32] {
        &self.driver.outputs()[channel]
    }
    fn latency_frames(&self) -> usize {
        self.latency_frames
    }
    fn supports_grab(&self) -> bool {
        true
    }
    fn metadata(&self) -> serde_json::Value {
        serde_json::json!({"driver":"stepped-dummy","numeric":"f32","timing":"host-wall-clock","processing_latency_frames":self.latency_frames,"page_bytes":1024,"resource_pages":MEMORY_BYTES/1024,"shared_take_capacity_bytes":SHARED_TAKE_PAGES as usize*1024,"smoothing_frames":0,"ramp_frames":0})
    }
}

pub fn prepare() -> Result<(Control<shoop_rt_engine::ControlEndpoint>, Cpu)> {
    let d = EngineDescription {
        quantum: QUANTUM,
        max_frames: QUANTUM,
        sample_rate: SAMPLE_RATE,
        host: HostPorts::numbered(2, 2, 0),
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
        midi_events: 1,
        midi_bytes: 16,
        history: host::InputHistorySettings {
            seconds: 1,
            audio_take_pages: SHARED_TAKE_PAGES,
            midi_pages: 16,
            midi_records: 16,
            writers: 128,
        },
        ..Default::default()
    };
    let (driver, endpoint, outbox) =
        SteppedDummy::prepare(&d).map_err(|e| anyhow!("CPU preparation: {e:?}"))?;
    let mut control = Control::new(endpoint, outbox)?;
    let pool_pages = driver.resource_usage().free_pages;
    let latency_frames = control.endpoint.limits().processing_latency_frames as usize;
    let mut cpu = Cpu {
        driver,
        pool_pages,
        latency_frames,
        input_origin: 0,
    };
    let strip = StripId(SlotId::new(0, 1));
    let mut requests = vec![Request::CreateStrip {
        strip,
        shape: StripShape {
            audio_inputs: 2,
            audio_outputs: 2,
            midi_input: false,
            midi_output: false,
        },
    }];
    for index in 0..2 {
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
