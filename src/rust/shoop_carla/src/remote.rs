use crate::{
    buffered::RenderEndpoint,
    native::{carla_runtime_availability, CarlaMainThreadUiDispatcher, CarlaMainThreadUiService},
    service::ChainService,
};
use anyhow::{Context, Result};
use rt_buffered::{AsyncBatch, Backend};
use rt_runtime::{buffer::MidiBuffer, Error};
use shoop_plugin_protocol::{CarlaChainType, CarlaProcessorLifecycle, ChainId};
use shoop_remote_fx::{Chain, ChainControls, Config, Dsp, Kind, Lifecycle, Status};
use std::{
    net::{SocketAddr, TcpListener},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

const CONTROL_TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_micros(100);

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
                make_chain(kind, config, worker.as_ref(), &dispatcher, &next)
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

fn make_chain(
    kind: Kind,
    config: Config,
    worker: Option<&PathBuf>,
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
    let limits = config.limits();
    let (endpoint, service) = if let Some(worker) = worker {
        ChainService::start_isolated(
            worker.clone(),
            ChainId(next.fetch_add(1, Ordering::Relaxed)),
            kind,
            config.sample_rate,
            timing,
            limits,
        )?
    } else {
        ChainService::start_in_process(kind, config.sample_rate, timing, limits, Some(ui.clone()))?
    };
    let chain = Arc::new(Owner {
        service,
        active: AtomicBool::new(false),
    });
    Ok(Chain {
        dsp: Box::new(RemoteDsp {
            endpoint,
            chain: Arc::clone(&chain),
        }),
        controls: Box::new(RemoteControls(chain)),
    })
}

struct Owner {
    service: ChainService,
    active: AtomicBool,
}

impl Owner {
    fn activate(&self, active: bool) -> Result<()> {
        self.service.set_active(active)?;
        self.active.store(active, Ordering::Release);
        Ok(())
    }
}

struct RemoteDsp {
    chain: Arc<Owner>,
    endpoint: RenderEndpoint,
}

impl Dsp for RemoteDsp {
    fn process(
        &mut self,
        input: AsyncBatch<'_>,
        audio: &mut [f32],
        midi: &mut [MidiBuffer],
    ) -> Result<(), Error> {
        if !self.chain.active.load(Ordering::Acquire) {
            self.chain.activate(true).map_err(|_| Error::Disconnected)?;
        }
        let tag = input.tag;
        let transport = self.endpoint.async_transport().ok_or(Error::InvalidState)?;
        transport.try_submit(input)?;
        self.chain.service.wake_audio();
        let timeout = Instant::now() + Duration::from_millis(10);
        while Instant::now() < timeout {
            if let Some(completion) = transport.try_completion() {
                if completion.tag == tag {
                    let output = completion.output?;
                    if output.audio.len() != audio.len() || output.midi.len() != midi.len() {
                        return Err(Error::InvalidShape);
                    }
                    audio.copy_from_slice(output.audio);
                    for (source, target) in output.midi.iter().zip(midi.iter_mut()) {
                        for index in 0..source.len() {
                            let (offset, data) = source.get(index).ok_or(Error::InvalidEvent)?;
                            target.push(offset, output.frames, data)?;
                        }
                    }
                    return Ok(());
                }
            } else {
                thread::sleep(POLL);
            }
        }
        Err(Error::Incomplete)
    }
}

struct RemoteControls(Arc<Owner>);

impl ChainControls for RemoteControls {
    fn status(&self) -> Status {
        let service = &self.0.service;
        let health = service.monitor().owned_health().1;
        Status {
            active: health.active && self.0.active.load(Ordering::Acquire),
            lifecycle: match health.lifecycle {
                CarlaProcessorLifecycle::Stopped => Lifecycle::Stopped,
                CarlaProcessorLifecycle::Starting => Lifecycle::Starting,
                CarlaProcessorLifecycle::Running => Lifecycle::Running,
                CarlaProcessorLifecycle::Degraded => Lifecycle::Degraded,
                CarlaProcessorLifecycle::Crashed => Lifecycle::Crashed,
                CarlaProcessorLifecycle::Restarting => Lifecycle::Restarting,
                CarlaProcessorLifecycle::Unavailable => Lifecycle::Unavailable,
            },
            editor_visible: service.editor().map(|editor| editor.is_visible()),
            ..Status::default()
        }
    }

    fn active(&mut self, active: bool) -> Result<(), String> {
        if self.0.active.load(Ordering::Acquire) == active {
            return Ok(());
        }
        self.0.activate(active).map_err(|error| error.to_string())
    }

    fn editor(&mut self, visible: bool) -> Result<(), String> {
        self.0
            .service
            .editor()
            .context("Carla editor unavailable")
            .and_then(|editor| editor.set_visible(visible))
            .map_err(|error| error.to_string())
    }

    fn save(&mut self) -> Result<String, String> {
        let service = &self.0.service;
        let (owner, health) = service.monitor().owned_health();
        let ticket = service
            .control()
            .capture_state(owner, health.generation)
            .map_err(|error| error.to_string())?;
        await_result(|| ticket.poll())
    }

    fn restore(&mut self, state: String) -> Result<(), String> {
        let service = &self.0.service;
        let (owner, health) = service.monitor().owned_health();
        let ticket = service
            .control()
            .restore_candidate(owner, health.generation, state)
            .map_err(|error| error.to_string())?;
        await_result(|| ticket.poll())
    }
}

fn await_result<T>(mut poll: impl FnMut() -> Option<Result<T>>) -> Result<T, String> {
    let end = Instant::now() + CONTROL_TIMEOUT;
    while Instant::now() < end {
        if let Some(result) = poll() {
            return result.map_err(|error| error.to_string());
        }
        thread::sleep(Duration::from_millis(1));
    }
    Err("Timed out waiting for Carla control".into())
}
