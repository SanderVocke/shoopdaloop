//! The render owner: exactly one runtime process call per host callback.

use crate::cells::Cells;
use crate::node::{EngineNode, EngineOp};
use rt_runtime::buffer::{AudioBlock, MidiBlock, MidiBuffer};
use rt_runtime::executor::GraphRuntime;
use rt_runtime::midi::ResetState;
use rt_runtime::resource::ResourceStore;
use rt_runtime::{Error, Frame};
use shoop_loops::runtime::LoopPlanner;
use std::sync::atomic::Ordering;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Running,
    /// The control endpoint is gone: output is silent after a MIDI reset.
    Disconnected,
    Faulted(Error),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Report {
    pub next_frame: Frame,
    pub phase: Phase,
    /// Packets waiting in the MIDI recovery outputs.
    pub recovery_packets: usize,
}

pub struct Render {
    runtime: GraphRuntime<EngineNode, LoopPlanner, EngineOp>,
    cells: Arc<Cells>,
    recovery: Box<[MidiBuffer]>,
    resets: Box<[ResetState]>,
    shape: [usize; 4],
    max_frames: usize,
    phase: Phase,
    reclaim_bound: usize,
}

impl Render {
    pub(crate) fn new(
        runtime: GraphRuntime<EngineNode, LoopPlanner, EngineOp>,
        cells: Arc<Cells>,
        shape: [usize; 4],
        max_frames: usize,
        reclaim_bound: usize,
    ) -> Result<Self, Error> {
        let outputs = shape[3];
        Ok(Self {
            runtime,
            cells,
            recovery: (0..outputs)
                .map(|_| MidiBuffer::new(2 * ResetState::EVENTS, 2 * ResetState::BYTES))
                .collect::<Result<_, _>>()?,
            resets: vec![ResetState::default(); outputs].into_boxed_slice(),
            shape,
            max_frames,
            phase: Phase::Running,
            reclaim_bound,
        })
    }
    pub fn next_frame(&self) -> Frame {
        self.runtime.next_frame()
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    /// The reserved device-recovery output of a host MIDI port, separate from
    /// ordinary output. Send it before the next callback.
    pub fn midi_recovery(&self, port: usize) -> Option<&MidiBuffer> {
        self.recovery.get(port)
    }
    pub fn diagnostics(&self) -> rt_runtime::executor::Diagnostics {
        self.runtime.diagnostics()
    }
    pub fn resource_usage(&self) -> rt_runtime::resource::PoolUsage {
        self.runtime.resource_usage()
    }
    fn report(&self) -> Report {
        Report {
            next_frame: self.next_frame(),
            phase: self.phase,
            recovery_packets: self.recovery.iter().map(MidiBuffer::len).sum(),
        }
    }
    fn drain_resets(&mut self, frames: usize) -> Result<(), Error> {
        for (reset, output) in self.resets.iter_mut().zip(self.recovery.iter_mut()) {
            for _ in 0..ResetState::EVENTS {
                let Some(packet) = reset.peek() else {
                    break;
                };
                output.push(0, frames, &packet)?;
                reset.acknowledge()?;
            }
        }
        Ok(())
    }
    fn request_resets(&mut self) {
        for reset in self.resets.iter_mut() {
            reset.request();
        }
    }
    fn silence(audio: &mut AudioBlock<'_, '_>, midi: &mut MidiBlock<'_>, frames: usize) {
        for output in audio.outputs.iter_mut() {
            output[..frames].fill(0.);
        }
        for output in midi.outputs.iter_mut() {
            output.clear();
        }
    }

    /// Render one host callback starting at `next_frame`. Host buffers have the
    /// prepared host port counts. On error the callback must not be replayed:
    /// stop callbacks and extract the stopped owners.
    pub fn process(
        &mut self,
        mut audio: AudioBlock<'_, '_>,
        mut midi: MidiBlock<'_>,
    ) -> Result<Report, Error> {
        let frames = audio.frames;
        if frames > self.max_frames
            || audio.inputs.len() != self.shape[0]
            || audio.outputs.len() != self.shape[1]
            || midi.inputs.len() != self.shape[2]
            || midi.outputs.len() != self.shape[3]
            || audio.inputs.iter().any(|x| x.len() < frames)
            || audio.outputs.iter().any(|x| x.len() < frames)
        {
            return Err(Error::InvalidShape);
        }
        for output in self.recovery.iter_mut() {
            output.clear();
        }
        if frames == 0 {
            return Ok(self.report());
        }
        self.cells.callbacks.fetch_add(1, Ordering::Relaxed);
        if self.phase == Phase::Running && !self.cells.endpoint_alive.load(Ordering::Relaxed) {
            self.phase = Phase::Disconnected;
            self.request_resets();
        }
        if self.phase != Phase::Running {
            Self::silence(&mut audio, &mut midi, frames);
            self.drain_resets(frames)?;
            return match self.phase {
                Phase::Faulted(error) => Err(error),
                _ => Ok(self.report()),
            };
        }
        if audio.frame != self.runtime.next_frame() {
            return Err(Error::InvalidState);
        }
        let result = self.runtime.process(
            AudioBlock {
                frame: audio.frame,
                frames,
                inputs: audio.inputs,
                outputs: &mut *audio.outputs,
            },
            MidiBlock {
                inputs: midi.inputs,
                outputs: &mut *midi.outputs,
            },
        );
        let recovery = (0..self.recovery.len()).try_for_each(|port| {
            self.runtime
                .drain_midi_recovery(port, &mut self.recovery[port], frames)
                .map(|_| ())
        });
        self.cells
            .next_frame
            .store(self.runtime.next_frame().0, Ordering::Relaxed);
        let peak = |buffers: &mut dyn Iterator<Item = &[f32]>| {
            buffers.fold(0f32, |peak, buffer| {
                buffer[..frames]
                    .iter()
                    .fold(peak, |peak, sample| peak.max(sample.abs()))
            })
        };
        self.cells
            .host_peaks
            .raise(0, peak(&mut audio.inputs.iter().copied()));
        self.cells
            .host_peaks
            .raise(1, peak(&mut audio.outputs.iter().map(|output| &**output)));
        if let Err(error) = result.and(recovery) {
            self.phase = Phase::Faulted(error);
            self.cells.faulted.store(true, Ordering::Relaxed);
            self.request_resets();
            Self::silence(&mut audio, &mut midi, frames);
            self.drain_resets(frames)?;
            return Err(error);
        }
        Ok(self.report())
    }

    /// Only after the host has stopped callbacks.
    pub fn into_stopped(self) -> Stopped {
        let (planner, resources) = self.runtime.into_planner_and_resources();
        Stopped {
            planner,
            resources,
            reclaim_bound: self.reclaim_bound,
        }
    }
}

/// The planner and resource store of a stopped engine, for off-render cleanup.
pub struct Stopped {
    planner: LoopPlanner,
    resources: ResourceStore,
    reclaim_bound: usize,
}

impl Stopped {
    pub fn resource_usage(&self) -> rt_runtime::resource::PoolUsage {
        self.resources.usage()
    }
    pub fn media_usage(&self) -> shoop_loops::media::Usage {
        self.planner.media_usage()
    }
    pub fn capture_group_usage(&mut self) -> rt_runtime::resource::CaptureGroupUsage {
        self.resources.capture_group_usage()
    }
    /// Finite, repeatable cleanup; it does not settle unrendered time.
    pub fn cleanup(&mut self) -> Result<(), Error> {
        self.planner.cleanup(&mut self.resources)?;
        for _ in 0..self.reclaim_bound {
            self.resources.begin_callback();
            self.resources.reclaim();
        }
        Ok(())
    }
}
