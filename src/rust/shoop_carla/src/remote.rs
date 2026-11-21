use crate::{
    native::{
        carla_runtime_availability, CarlaMainThreadUiDispatcher, CarlaMainThreadUiService,
        CarlaNativeHost,
    },
    processor::{CarlaExternalUi, CarlaProcessor},
    subprocess::SupervisedCarlaProcessor,
};
use anyhow::{Context, Result};
use rt_buffered::{AsyncBatch, Batch, BatchTag};
use rt_runtime::{
    buffer::{AudioPorts, MidiBuffer},
    Error,
};
use shoop_plugin_protocol::{
    CarlaChainType, CarlaProcessorLifecycle, ChainId, PreparedMidiBuffer,
    MAX_MIDI_EVENTS_PER_BLOCK, MAX_NATIVE_MIDI_PACKET_BYTES,
};
use shoop_remote_fx::{Chain, ChainControls, Config, Dsp, Kind, Lifecycle, Status};
use std::{
    net::{SocketAddr, TcpListener},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

pub fn serve(address: SocketAddr, slots: usize, worker: Option<PathBuf>) -> Result<()> {
    carla_runtime_availability().map_err(anyhow::Error::msg)?;
    let listener = TcpListener::bind(address)?;
    let (mut ui, dispatcher) = CarlaMainThreadUiService::new();
    let stopping = Arc::new(AtomicBool::new(false));
    let next = AtomicU64::new(1);
    println!("FX service listening on {}", listener.local_addr()?);
    let broker = thread::Builder::new()
        .name("remote-fx-service".into())
        .spawn(move || {
            shoop_remote_fx::serve(listener, slots, stopping, move |kind, config| {
                make_chain(kind, config, worker.as_deref(), &dispatcher, &next)
                    .map_err(|error| error.to_string())
            })
        })?;
    while !broker.is_finished() {
        ui.pump();
        thread::sleep(Duration::from_millis(1));
    }
    broker
        .join()
        .map_err(|_| anyhow::anyhow!("FX service broker panicked"))??;
    Ok(())
}

type Processor = Arc<Mutex<Box<dyn CarlaProcessor>>>;

fn make_chain(
    kind: Kind,
    config: Config,
    worker: Option<&Path>,
    ui: &CarlaMainThreadUiDispatcher,
    next: &AtomicU64,
) -> Result<Chain> {
    let kind = match kind {
        Kind::Rack => CarlaChainType::Rack,
        Kind::Patchbay => CarlaChainType::Patchbay,
        Kind::Patchbay16x => CarlaChainType::Patchbay16x,
    };
    let timing = config
        .prepare()
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let frames = timing.batch_frames() as u32;
    let mut processor: Box<dyn CarlaProcessor> = if let Some(worker) = worker {
        Box::new(SupervisedCarlaProcessor::launch(
            worker.to_owned(),
            kind,
            config.sample_rate,
            frames,
            ChainId(next.fetch_add(1, Ordering::Relaxed)),
        )?)
    } else {
        Box::new(CarlaNativeHost::instantiate_with_ui_dispatcher(
            kind,
            config.sample_rate,
            frames,
            ui.clone(),
        )?)
    };
    anyhow::ensure!(processor.is_ready(), "Carla processor did not start");
    let editor = processor.external_ui();
    let processor = Arc::new(Mutex::new(processor));
    Ok(Chain {
        dsp: Box::new(RemoteDsp {
            processor: Arc::clone(&processor),
            previous: None,
            midi: PreparedMidiBuffer::new(MAX_MIDI_EVENTS_PER_BLOCK, MAX_NATIVE_MIDI_PACKET_BYTES),
        }),
        controls: Box::new(RemoteControls { processor, editor }),
    })
}

struct RemoteDsp {
    processor: Processor,
    previous: Option<(BatchTag, bool)>,
    midi: PreparedMidiBuffer,
}

impl Dsp for RemoteDsp {
    fn process(
        &mut self,
        input: AsyncBatch<'_>,
        output: &mut [f32],
        midi: &mut [MidiBuffer],
    ) -> Result<(), Error> {
        // A fresh Carla instance has no prior MIDI or audio to flush. Preserve the
        // restored chain's activation when the first batch arrives after a reconnect.
        let discontinuous = self.previous.is_some_and(|(previous, failed)| {
            failed
                || previous.generation != input.tag.generation
                || previous.sequence.checked_add(1) != Some(input.tag.sequence)
                || previous.source.checked_add(input.frames).ok() != Some(input.tag.source)
        });
        let mut processor = self.processor.lock().map_err(|_| Error::Disconnected)?;
        let result = crate::buffered::process(
            processor.as_mut(),
            &mut self.midi,
            discontinuous,
            &mut Batch {
                tag: input.tag,
                frames: input.frames,
                audio: AudioPorts::new(input.audio, output, input.frames, 0, input.frames)?,
                midi_inputs: input.midi,
                midi_controls: None,
                midi_outputs: midi,
                parameters: input.parameters,
                observer: None,
            },
        );
        self.previous = Some((input.tag, result.is_err()));
        result
    }
}

struct RemoteControls {
    processor: Processor,
    editor: Option<Arc<dyn CarlaExternalUi>>,
}

impl ChainControls for RemoteControls {
    fn status(&self) -> Status {
        let Ok(mut processor) = self.processor.lock() else {
            return Status {
                lifecycle: Lifecycle::Crashed,
                ..Status::default()
            };
        };
        processor.idle();
        let _ = processor.is_ready();
        Status {
            active: processor.is_active(),
            lifecycle: match processor.lifecycle() {
                CarlaProcessorLifecycle::Stopped => Lifecycle::Stopped,
                CarlaProcessorLifecycle::Starting => Lifecycle::Starting,
                CarlaProcessorLifecycle::Running => Lifecycle::Running,
                CarlaProcessorLifecycle::Degraded => Lifecycle::Degraded,
                CarlaProcessorLifecycle::Crashed => Lifecycle::Crashed,
                CarlaProcessorLifecycle::Restarting => Lifecycle::Restarting,
                CarlaProcessorLifecycle::Unavailable => Lifecycle::Unavailable,
            },
            editor_visible: self.editor.as_ref().map(|editor| editor.is_visible()),
            ..Status::default()
        }
    }

    fn active(&mut self, active: bool) -> Result<(), String> {
        let mut processor = self.processor.lock().map_err(|error| error.to_string())?;
        if processor.is_active() != active {
            processor.set_active(active);
        }
        if processor.is_active() != active || !processor.is_ready() {
            return Err("Carla active state was not confirmed".into());
        }
        Ok(())
    }

    fn editor(&mut self, visible: bool) -> Result<(), String> {
        self.editor
            .as_ref()
            .context("Carla editor unavailable")
            .and_then(|editor| editor.set_visible(visible))
            .map_err(|error| error.to_string())
    }

    fn save(&mut self) -> Result<String, String> {
        let mut processor = self.processor.lock().map_err(|error| error.to_string())?;
        processor.capture_state().map_err(|error| error.to_string())
    }

    fn restore(&mut self, state: String) -> Result<(), String> {
        let mut processor = self.processor.lock().map_err(|error| error.to_string())?;
        crate::state::restore_candidate(processor.as_mut(), &state)
            .map_err(|error| error.to_string())
    }
}
