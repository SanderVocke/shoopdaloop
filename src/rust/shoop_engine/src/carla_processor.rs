use crate::realtime_lock_guard::Mutex;
use crate::FXChainType;
use anyhow::Result;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CarlaProcessorInfo {
    pub chain_type: FXChainType,
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    pub midi_inputs: usize,
    pub midi_outputs: usize,
}

pub type SharedCarlaProcessor = Arc<Mutex<Box<dyn CarlaProcessor>>>;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CarlaProcessorLifecycle {
    #[default]
    Stopped,
    Starting,
    Running,
    Crashed,
    Restarting,
    Unavailable,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CarlaGenerationLog {
    pub generation: u64,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_dropped_bytes: u64,
    pub stderr_dropped_bytes: u64,
}

pub trait CarlaProcessor: Send + Debug {
    fn info(&self) -> CarlaProcessorInfo;
    fn is_ready(&mut self) -> bool {
        true
    }
    fn lifecycle(&self) -> CarlaProcessorLifecycle {
        CarlaProcessorLifecycle::Running
    }
    fn generation(&self) -> u64 {
        0
    }
    fn crash_summary(&self) -> Option<&str> {
        None
    }
    fn generation_logs(&self) -> Vec<CarlaGenerationLog> {
        Vec::new()
    }
    fn clear_logs(&mut self) {}
    fn toggle_or_recover(&mut self) -> Result<()> {
        let visible = self.is_visible();
        self.set_visible(!visible)
    }
    fn set_active(&mut self, active: bool);
    fn is_active(&self) -> bool;
    fn set_visible(&mut self, visible: bool) -> Result<()>;
    fn is_visible(&mut self) -> bool;
    fn save_state(&mut self) -> Result<String>;
    fn restore_state(&mut self, state: &str) -> Result<()>;
    fn audio_input_mut(&mut self, index: usize) -> Option<&mut [f32]>;
    fn audio_output(&self, index: usize) -> Option<&[f32]>;
    fn set_midi_input_events(&mut self, index: usize, events: &[(u32, &[u8])]) -> Result<()>;
    fn midi_output_events(&mut self, index: usize) -> Result<Vec<(u32, Vec<u8>)>>;
    fn process(&mut self, frames: usize) -> Result<()>;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FakeProcessorBehavior {
    pub process_delay: Duration,
    pub fail_processing: bool,
    pub panic_processing: bool,
    pub fail_state: bool,
    pub fail_visibility: bool,
}

#[derive(Debug)]
pub struct FakeCarlaProcessor {
    info: CarlaProcessorInfo,
    audio_inputs: Vec<Vec<f32>>,
    audio_outputs: Vec<Vec<f32>>,
    midi_inputs: Vec<Vec<(u32, Vec<u8>)>>,
    midi_outputs: Vec<Vec<(u32, Vec<u8>)>>,
    active: bool,
    visible: bool,
    state: String,
    behavior: FakeProcessorBehavior,
}

impl FakeCarlaProcessor {
    pub fn new(chain_type: FXChainType, audio_channels: usize, max_frames: usize) -> Self {
        Self {
            info: CarlaProcessorInfo {
                chain_type,
                audio_inputs: audio_channels,
                audio_outputs: audio_channels,
                midi_inputs: 1,
                midi_outputs: 1,
            },
            audio_inputs: vec![vec![0.0; max_frames]; audio_channels],
            audio_outputs: vec![vec![0.0; max_frames]; audio_channels],
            midi_inputs: vec![Vec::new()],
            midi_outputs: vec![Vec::new()],
            active: false,
            visible: false,
            state: "{}".to_owned(),
            behavior: FakeProcessorBehavior::default(),
        }
    }

    pub fn set_behavior(&mut self, behavior: FakeProcessorBehavior) {
        self.behavior = behavior;
    }
}

impl CarlaProcessor for FakeCarlaProcessor {
    fn info(&self) -> CarlaProcessorInfo {
        self.info
    }

    fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn set_visible(&mut self, visible: bool) -> Result<()> {
        if self.behavior.fail_visibility {
            anyhow::bail!("fake visibility failure");
        }
        self.visible = visible;
        Ok(())
    }

    fn is_visible(&mut self) -> bool {
        self.visible
    }

    fn save_state(&mut self) -> Result<String> {
        if self.behavior.fail_state {
            anyhow::bail!("fake state save failure");
        }
        Ok(self.state.clone())
    }

    fn restore_state(&mut self, state: &str) -> Result<()> {
        if self.behavior.fail_state {
            anyhow::bail!("fake state restore failure");
        }
        self.state = state.to_owned();
        Ok(())
    }

    fn audio_input_mut(&mut self, index: usize) -> Option<&mut [f32]> {
        self.audio_inputs.get_mut(index).map(Vec::as_mut_slice)
    }

    fn audio_output(&self, index: usize) -> Option<&[f32]> {
        self.audio_outputs.get(index).map(Vec::as_slice)
    }

    fn set_midi_input_events(&mut self, index: usize, events: &[(u32, &[u8])]) -> Result<()> {
        let destination = self
            .midi_inputs
            .get_mut(index)
            .ok_or_else(|| anyhow::anyhow!("no fake MIDI input {index}"))?;
        destination.clear();
        destination.extend(events.iter().map(|(offset, data)| (*offset, data.to_vec())));
        Ok(())
    }

    fn midi_output_events(&mut self, index: usize) -> Result<Vec<(u32, Vec<u8>)>> {
        self.midi_outputs
            .get(index)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no fake MIDI output {index}"))
    }

    fn process(&mut self, frames: usize) -> Result<()> {
        if self.behavior.panic_processing {
            panic!("fake processor panic");
        }
        if !self.behavior.process_delay.is_zero() {
            std::thread::sleep(self.behavior.process_delay);
        }
        if self.behavior.fail_processing {
            anyhow::bail!("fake processing failure");
        }
        if !self.active {
            return Ok(());
        }
        for (input, output) in self.audio_inputs.iter().zip(&mut self.audio_outputs) {
            let frames = frames.min(input.len()).min(output.len());
            output[..frames].copy_from_slice(&input[..frames]);
        }
        for (input, output) in self.midi_inputs.iter().zip(&mut self.midi_outputs) {
            output.clone_from(input);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_processor_round_trips_audio_midi_state_and_visibility() {
        let mut processor = FakeCarlaProcessor::new(FXChainType::CarlaRack, 2, 64);
        processor.set_active(true);
        processor.set_visible(true).unwrap();
        processor.restore_state("checkpoint").unwrap();
        processor.audio_input_mut(0).unwrap()[..4].copy_from_slice(&[1.0, 2.0, 3.0, 4.0]);
        processor
            .set_midi_input_events(0, &[(3, &[0x90, 60, 100])])
            .unwrap();
        processor.process(4).unwrap();

        assert_eq!(
            processor.audio_output(0).unwrap()[..4],
            [1.0, 2.0, 3.0, 4.0]
        );
        assert_eq!(
            processor.midi_output_events(0).unwrap(),
            vec![(3, vec![0x90, 60, 100])]
        );
        assert_eq!(processor.save_state().unwrap(), "checkpoint");
        assert!(processor.is_visible());
    }

    #[test]
    fn fake_processor_exposes_failures_and_delay() {
        let mut processor = FakeCarlaProcessor::new(FXChainType::CarlaRack, 2, 64);
        processor.set_behavior(FakeProcessorBehavior {
            process_delay: Duration::from_millis(1),
            fail_processing: true,
            fail_state: true,
            fail_visibility: true,
            ..Default::default()
        });
        let started = std::time::Instant::now();
        assert!(processor.process(4).is_err());
        assert!(started.elapsed() >= Duration::from_millis(1));
        assert!(processor.save_state().is_err());
        assert!(processor.restore_state("state").is_err());
        assert!(processor.set_visible(true).is_err());
    }
}
