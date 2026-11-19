//! Legacy adapters for the shared Carla child and parent host.

use crate::{
    carla_processor::{
        CarlaExternalUi, CarlaGenerationLog, CarlaMidiBuffer, CarlaProcessor, CarlaProcessorInfo,
        CarlaProcessorLifecycle, FakeCarlaProcessor, FakeProcessorBehavior,
    },
    FXChainType,
};
use anyhow::Result;
use shoop_carla::{
    child::ChildPlugin,
    processor::{CarlaExternalUi as NativeEditor, CarlaProcessor as NativeProcessor},
    subprocess as parent,
};
use shoop_plugin_protocol::{
    CarlaChainType, ChainId, ProcessGeneration, WorkerExitKind, MAX_BLOCK_FRAMES,
};
use std::{
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

pub use shoop_carla::child::{
    parse_nonce, worker_options, CarlaWorkerOptions, CarlaWorkerTestMode,
};
pub use shoop_carla::subprocess::LogSnapshot;

fn engine_chain_type(chain_type: CarlaChainType) -> FXChainType {
    match chain_type {
        CarlaChainType::Rack => FXChainType::CarlaRack,
        CarlaChainType::Patchbay => FXChainType::CarlaPatchbay,
        CarlaChainType::Patchbay16x => FXChainType::CarlaPatchbay16x,
    }
}

pub fn run_carla_worker(options: CarlaWorkerOptions) -> Result<()> {
    shoop_carla::child::run_with_factory(options, |mode, chain_type| {
        let mut fake = FakeCarlaProcessor::new(
            engine_chain_type(chain_type),
            chain_type.audio_channels() as usize,
            MAX_BLOCK_FRAMES,
        );
        let behavior = match mode {
            CarlaWorkerTestMode::Abort => FakeProcessorBehavior {
                panic_processing: true,
                ..Default::default()
            },
            CarlaWorkerTestMode::ProcessError => FakeProcessorBehavior {
                fail_processing: true,
                ..Default::default()
            },
            CarlaWorkerTestMode::Hang => FakeProcessorBehavior {
                process_delay: Duration::from_secs(5),
                ..Default::default()
            },
            CarlaWorkerTestMode::DelayOnce => FakeProcessorBehavior {
                process_delay: Duration::from_millis(500),
                process_delay_calls: Some(1),
                ..Default::default()
            },
            CarlaWorkerTestMode::UiClose => FakeProcessorBehavior {
                close_ui_immediately: true,
                ..Default::default()
            },
            _ => FakeProcessorBehavior::default(),
        };
        fake.set_behavior(behavior);
        Ok(Box::new(fake))
    })
}

impl ChildPlugin for FakeCarlaProcessor {
    fn set_active(&mut self, active: bool) {
        CarlaProcessor::set_active(self, active)
    }

    fn is_active(&self) -> bool {
        CarlaProcessor::is_active(self)
    }

    fn set_visible(&mut self, visible: bool) -> Result<()> {
        CarlaProcessor::set_visible(self, visible)
    }

    fn is_visible(&mut self) -> bool {
        CarlaProcessor::is_visible(self)
    }

    fn save_state(&mut self) -> Result<String> {
        CarlaProcessor::save_state(self)
    }

    fn restore_state(&mut self, state: &str) -> Result<()> {
        CarlaProcessor::restore_state(self, state)
    }

    fn audio_input_mut(&mut self, channel: usize) -> Option<&mut [f32]> {
        CarlaProcessor::audio_input_mut(self, channel)
    }

    fn audio_output(&self, channel: usize) -> Option<&[f32]> {
        CarlaProcessor::audio_output(self, channel)
    }

    fn process(&mut self, frames: usize) -> Result<()> {
        CarlaProcessor::process(self, frames)
    }

    fn set_midi_input(&mut self, events: &[(u32, &[u8])]) -> Result<()> {
        self.set_midi_input_events(0, events)
    }

    fn fill_midi_output(&mut self, output: &mut CarlaMidiBuffer) -> Result<()> {
        self.fill_midi_output_events(0, output)
    }
}

#[derive(Debug)]
struct ExternalUi(Arc<dyn NativeEditor>);

impl CarlaExternalUi for ExternalUi {
    fn set_visible(&self, visible: bool) -> Result<()> {
        self.0.set_visible(visible)
    }

    fn refresh(&self) {
        self.0.refresh();
    }

    fn is_visible(&self) -> bool {
        self.0.is_visible()
    }

    fn ui_was_closed(&self) -> bool {
        self.0.ui_was_closed()
    }
}

macro_rules! legacy_processor {
    ($name:ident) => {
        #[derive(Debug)]
        pub struct $name(parent::$name);

        impl Deref for $name {
            type Target = parent::$name;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl DerefMut for $name {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.0
            }
        }

        impl CarlaProcessor for $name {
            fn info(&self) -> CarlaProcessorInfo {
                let info = self.0.info();
                CarlaProcessorInfo {
                    chain_type: engine_chain_type(info.chain_type),
                    audio_inputs: info.audio_inputs,
                    audio_outputs: info.audio_outputs,
                    midi_inputs: info.midi_inputs,
                    midi_outputs: info.midi_outputs,
                }
            }

            fn external_ui(&self) -> Option<Arc<dyn CarlaExternalUi>> {
                self.0
                    .external_ui()
                    .map(|ui| Arc::new(ExternalUi(ui)) as Arc<dyn CarlaExternalUi>)
            }

            fn is_ready(&mut self) -> bool {
                self.0.is_ready()
            }

            fn lifecycle(&self) -> CarlaProcessorLifecycle {
                self.0.lifecycle()
            }

            fn generation(&self) -> u64 {
                self.0.generation()
            }

            fn exit_kind(&self) -> WorkerExitKind {
                self.0.exit_kind()
            }

            fn crash_summary(&self) -> Option<String> {
                self.0.crash_summary()
            }

            fn deadline_misses(&self) -> u64 {
                self.0.deadline_misses()
            }

            fn stale_completions(&self) -> u64 {
                self.0.stale_completions()
            }

            fn recoveries(&self) -> u64 {
                self.0.recoveries()
            }

            fn generation_logs(&self) -> Vec<CarlaGenerationLog> {
                self.0.generation_logs()
            }

            fn clear_logs(&mut self) {
                self.0.clear_logs()
            }

            fn idle(&mut self) {
                self.0.idle()
            }

            fn toggle_or_recover(&mut self) -> Result<()> {
                self.0.toggle_or_recover()
            }

            fn set_active(&mut self, active: bool) {
                self.0.set_active(active)
            }

            fn is_active(&self) -> bool {
                self.0.is_active()
            }

            fn set_visible(&mut self, visible: bool) -> Result<()> {
                self.0.set_visible(visible)
            }

            fn is_visible(&mut self) -> bool {
                self.0.is_visible()
            }

            fn save_state(&mut self) -> Result<String> {
                self.0.save_state()
            }

            fn restore_state(&mut self, state: &str) -> Result<()> {
                self.0.restore_state(state)
            }

            fn audio_input_mut(&mut self, index: usize) -> Option<&mut [f32]> {
                self.0.audio_input_mut(index)
            }

            fn audio_output(&self, index: usize) -> Option<&[f32]> {
                self.0.audio_output(index)
            }

            fn set_midi_input_events(
                &mut self,
                index: usize,
                events: &[(u32, &[u8])],
            ) -> Result<()> {
                self.0.set_midi_input_events(index, events)
            }

            fn midi_output_events(&mut self, index: usize) -> Result<Vec<(u32, Vec<u8>)>> {
                self.0.midi_output_events(index)
            }

            fn fill_midi_output_events(
                &mut self,
                index: usize,
                destination: &mut CarlaMidiBuffer,
            ) -> Result<()> {
                self.0.fill_midi_output_events(index, destination)
            }

            fn process(&mut self, frames: usize) -> Result<()> {
                self.0.process(frames)
            }
        }
    };
}
legacy_processor!(SubprocessCarlaProcessor);
legacy_processor!(SupervisedCarlaProcessor);

impl SubprocessCarlaProcessor {
    pub fn spawn(
        executable: impl AsRef<Path>,
        chain_type: FXChainType,
        sample_rate: u32,
        nominal_buffer_size: u32,
        chain_id: ChainId,
        generation: ProcessGeneration,
    ) -> Result<Self> {
        parent::SubprocessCarlaProcessor::spawn(
            executable,
            chain_type.try_into()?,
            sample_rate,
            nominal_buffer_size,
            chain_id,
            generation,
        )
        .map(Self)
    }

    pub fn spawn_test_worker(
        executable: impl AsRef<Path>,
        chain_type: FXChainType,
        sample_rate: u32,
        nominal_buffer_size: u32,
        chain_id: ChainId,
        generation: ProcessGeneration,
        test_mode: CarlaWorkerTestMode,
    ) -> Result<Self> {
        parent::SubprocessCarlaProcessor::spawn_test_worker(
            executable,
            chain_type.try_into()?,
            sample_rate,
            nominal_buffer_size,
            chain_id,
            generation,
            test_mode,
        )
        .map(Self)
    }

    pub fn current_executable() -> Result<PathBuf> {
        parent::SubprocessCarlaProcessor::current_executable()
    }
}

impl SupervisedCarlaProcessor {
    pub fn launch(
        executable: impl Into<PathBuf>,
        chain_type: FXChainType,
        sample_rate: u32,
        nominal_buffer_size: u32,
        chain_id: ChainId,
    ) -> Result<Self> {
        parent::SupervisedCarlaProcessor::launch(
            executable,
            chain_type.try_into()?,
            sample_rate,
            nominal_buffer_size,
            chain_id,
        )
        .map(Self)
    }

    pub fn launch_test_worker(
        executable: impl Into<PathBuf>,
        chain_type: FXChainType,
        sample_rate: u32,
        nominal_buffer_size: u32,
        chain_id: ChainId,
        test_mode: CarlaWorkerTestMode,
    ) -> Result<Self> {
        parent::SupervisedCarlaProcessor::launch_test_worker(
            executable,
            chain_type.try_into()?,
            sample_rate,
            nominal_buffer_size,
            chain_id,
            test_mode,
        )
        .map(Self)
    }
}
