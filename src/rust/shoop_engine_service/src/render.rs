use crate::{control::Ingress, dsp, Config, Phase, Report};
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    midi::ResetState,
    Error, Frame,
};
use rt_runtime::{executor::GraphRuntime, resource::ResourceStore};
use shoop_loops::runtime::LoopPlanner;

pub struct Render {
    engine: GraphRuntime<crate::ServiceNode, LoopPlanner, dsp::Operation>,
    ingress: Ingress,
    recovery: MidiBuffer,
    reset: ResetState,
    config: Config,
    phase: Phase,
}

impl Render {
    pub(crate) fn new(
        config: Config,
        engine: GraphRuntime<crate::ServiceNode, LoopPlanner, dsp::Operation>,
        ingress: Ingress,
    ) -> Result<Self, Error> {
        Ok(Self {
            engine,
            ingress,
            recovery: MidiBuffer::new(64, 192)?,
            reset: ResetState::default(),
            config,
            phase: Phase::Running,
        })
    }
    pub fn next_frame(&self) -> Frame {
        self.engine.next_frame()
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    /// Separate reserved device-recovery output; consume before the next callback.
    pub fn midi_recovery(&self) -> &MidiBuffer {
        &self.recovery
    }
    pub fn diagnostics(&self) -> rt_runtime::executor::Diagnostics {
        self.engine.diagnostics()
    }
    pub fn resource_usage(&self) -> rt_runtime::resource::PoolUsage {
        self.engine.resource_usage()
    }
    fn report(&self) -> Report {
        Report {
            next_frame: self.next_frame(),
            phase: self.phase,
            recovery_packets: self.recovery.len(),
            work: self.ingress.work,
        }
    }
    fn drain_reset(&mut self, frames: usize) -> Result<(), Error> {
        for _ in 0..ResetState::EVENTS {
            let Some(packet) = self.reset.peek() else {
                break;
            };
            self.recovery.push(0, frames, &packet)?;
            self.reset.acknowledge()?;
        }
        Ok(())
    }
    /// Exactly one engine process call for each valid running nonempty host block.
    /// On error, do not replay: stop callbacks and extract quiescent ownership.
    pub fn process(
        &mut self,
        audio: AudioBlock<'_, '_>,
        midi: MidiBlock<'_>,
    ) -> Result<Report, Error> {
        let frames = audio.frames;
        if frames > self.config.max_frames
            || audio.inputs.len() != 2
            || audio.outputs.len() != 2
            || midi.inputs.len() != 1
            || midi.outputs.len() != 1
            || audio.inputs.iter().any(|x| x.len() < frames)
            || audio.outputs.iter().any(|x| x.len() < frames)
        {
            return Err(Error::InvalidShape);
        }
        self.recovery.clear();
        for output in midi.outputs.iter_mut() {
            output.clear();
        }
        if frames == 0 {
            let mut report = self.report();
            report.work = crate::Work::default();
            return Ok(report);
        }
        if matches!(self.phase, Phase::Quiescent | Phase::Faulted(_)) {
            for output in audio.outputs.iter_mut() {
                output[..frames].fill(0.);
            }
            self.drain_reset(frames)?;
            return match self.phase {
                Phase::Faulted(error) => Err(error),
                _ => Ok(self.report()),
            };
        }
        if audio.frame != self.engine.next_frame() {
            return Err(Error::InvalidState);
        }
        let frame = audio.frame;
        let ingress = self.ingress.begin(frame, self.engine.diagnostics().epoch);
        if ingress == Ok(true) {
            self.phase = Phase::Disconnecting;
            self.reset.request();
        }
        let processing = ingress.and_then(|_| {
            self.engine.process(
                AudioBlock {
                    frame,
                    frames,
                    inputs: audio.inputs,
                    outputs: &mut *audio.outputs,
                },
                MidiBlock {
                    inputs: midi.inputs,
                    outputs: &mut *midi.outputs,
                },
            )
        });
        let outcomes = self.ingress.finish();
        let recovery = self
            .engine
            .drain_midi_recovery(0, &mut self.recovery, frames)
            .map(|_| ());
        let result = processing.and(outcomes).and(recovery);
        if let Err(error) = result {
            self.phase = Phase::Faulted(error);
            self.reset.request();
            let aborted = self.ingress.abort(frame, error);
            for output in audio.outputs.iter_mut() {
                output[..frames].fill(0.);
            }
            for output in midi.outputs.iter_mut() {
                output.clear();
            }
            self.drain_reset(frames)?;
            aborted?;
            return Err(error);
        }
        if self.ingress.disconnected {
            for output in audio.outputs.iter_mut() {
                output[..frames].fill(0.);
            }
            for output in midi.outputs.iter_mut() {
                output.clear();
            }
        }
        self.drain_reset(frames)?;
        if self.ingress.stop_failed {
            self.phase = Phase::Faulted(Error::InvalidState);
            return Err(Error::InvalidState);
        }
        if self.ingress.stop_applied && !self.reset.pending() {
            self.phase = Phase::Quiescent;
        }
        Ok(self.report())
    }
    /// Only after the host has stopped callbacks. Undeliverable owners are then
    /// disposed off-render, never by treating a disconnected UI as a free sink.
    pub fn into_stopped(self) -> Stopped {
        let (planner, resources) = self.engine.into_planner_and_resources();
        Stopped {
            planner,
            resources,
            reclaim_bound: self.config.resource_pages.saturating_add(128),
        }
    }
}

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
    pub fn midi_index_usage(&mut self) -> rt_runtime::resource::MidiIndexUsage {
        self.resources.midi_index_usage()
    }
    /// Finite off-render cleanup/reclamation; it does not settle unrendered time.
    pub fn cleanup(&mut self) -> Result<(), Error> {
        self.planner.cleanup(&mut self.resources)?;
        for _ in 0..self.reclaim_bound {
            self.resources.begin_callback();
            self.resources.reclaim();
        }
        Ok(())
    }
}
