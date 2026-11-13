use anyhow::{anyhow, ensure, Result};
use shoop_app::{AudioDriverController, DriverCompletion, DriverReply, DriverWork};
use shoop_app_api::*;
use shoop_rt_backend::{RtBackend, Sleep};
use shoop_rt_host_native::{NativeConfig, NativeHost};
use std::{
    sync::mpsc::{self, Receiver, SyncSender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

mod conversion;
#[cfg(test)]
mod tests;

pub type PreferenceWriter = Box<dyn FnMut(&AudioDriverConfig) -> Result<()> + Send>;

pub fn read_preferences(path: &std::path::Path) -> Result<Option<AudioDriverConfig>> {
    use std::io::Read;
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut bytes = Vec::new();
    file.take(65_537).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 65_536, "driver preference exceeds 64 KiB");
    let config: AudioDriverConfig = serde_json::from_slice(&bytes)?;
    ensure!(config.bounded(), "stored driver preference is invalid");
    Ok(Some(config))
}

pub fn preference_writer(path: Option<std::path::PathBuf>) -> PreferenceWriter {
    Box::new(move |config| {
        use std::io::Write;
        let path = path
            .as_ref()
            .ok_or_else(|| anyhow!("this host has no configured startup preference store"))?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(std::path::Path::new("."));
        std::fs::create_dir_all(parent)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&serde_json::to_vec(config)?)?;
        temporary.as_file().sync_all()?;
        temporary.persist(path).map_err(|e| e.error)?.sync_all()?;
        #[cfg(unix)]
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    })
}

enum Request {
    Work(u64, DriverWork),
    Stop(mpsc::Sender<Result<(), String>>),
}

pub struct Owner {
    sender: SyncSender<Request>,
    join: Option<JoinHandle<()>>,
    stopping: Option<Receiver<Result<(), String>>>,
    stop_result: Option<Result<(), String>>,
}
impl Owner {
    pub fn poll_stop(&mut self) -> Option<Result<(), String>> {
        if let Some(result) = &self.stop_result {
            return Some(result.clone());
        }
        if self.stopping.is_none() {
            let (tx, rx) = mpsc::channel();
            if self.sender.try_send(Request::Stop(tx)).is_err() {
                return Some(Err(
                    "audio lifecycle worker is busy or unavailable".to_owned()
                ));
            }
            self.stopping = Some(rx);
        }
        if self.join.as_ref().is_some_and(|join| !join.is_finished()) {
            return None;
        }
        let result = match self.stopping.as_ref().unwrap().try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("audio lifecycle worker lost its cleanup result".into())
            }
        };
        let result = if self.join.take().is_some_and(|join| join.join().is_err()) {
            Err("audio lifecycle worker panicked".into())
        } else {
            result
        };
        self.stopping = None;
        self.stop_result = Some(result.clone());
        Some(result)
    }

    pub fn stop(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(result) = self.poll_stop() {
                return result;
            }
            if Instant::now() >= deadline {
                return Err("audio lifecycle cleanup did not finish".into());
            }
            thread::sleep(Duration::from_millis(1));
        }
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
struct Controller {
    sender: SyncSender<Request>,
    receiver: Receiver<DriverCompletion>,
    ticket: Option<u64>,
}
impl AudioDriverController for Controller {
    fn submit(&mut self, ticket: u64, work: DriverWork) -> Result<(), String> {
        if ticket == 0 || self.ticket.is_some() {
            return Err("audio lifecycle work is already pending".into());
        }
        self.sender
            .try_send(Request::Work(ticket, work))
            .map_err(|_| "audio lifecycle worker is busy or unavailable".to_owned())?;
        self.ticket = Some(ticket);
        Ok(())
    }
    fn poll(&mut self) -> Option<DriverCompletion> {
        match self.receiver.try_recv() {
            Ok(result) => {
                self.ticket = None;
                Some(result)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.ticket.take().map(|ticket| DriverCompletion {
                    ticket,
                    result: Err("audio lifecycle worker disconnected".into()),
                })
            }
        }
    }
}

pub fn controller(
    host: NativeHost,
    settings: HostSettings,
    mut preferences: PreferenceWriter,
) -> Result<(Owner, Box<dyn AudioDriverController>)> {
    let (sender, requests) = mpsc::sync_channel(2);
    let (results, receiver) = mpsc::sync_channel(1);
    let join = thread::Builder::new()
        .name("shoop-audio-lifecycle".into())
        .spawn(move || {
            let mut host = Some(host);
            while let Ok(request) = requests.recv() {
                match request {
                    Request::Stop(reply) => {
                        let result = stop(&mut host).map_err(|e| e.to_string());
                        let _ = reply.send(result);
                        break;
                    }
                    Request::Work(ticket, work) => {
                        let result = match work {
                            DriverWork::Discover(config) => {
                                discover(&config).map(DriverReply::Discovered)
                            }
                            DriverWork::Preflight(config) => {
                                preflight(&config, &settings).map(DriverReply::Resolved)
                            }
                            DriverWork::Convert {
                                image,
                                target,
                                mut limits,
                            } => {
                                limits.media_bytes = limits
                                    .media_bytes
                                    .min(u64::from(settings.pools.loop_memory_mb) * (1 << 20));
                                validate_target_processors(&image, target, &settings)
                                    .and_then(|()| conversion::convert(image, target, limits))
                                    .map(DriverReply::Converted)
                            }
                            DriverWork::Persist(config) => {
                                preferences(&config).map(|()| DriverReply::Persisted)
                            }
                            DriverWork::Start {
                                profile,
                                exact_devices,
                            } => (|| {
                                stop(&mut host)?;
                                let mut native =
                                    strict_config(&profile.configured, &settings, exact_devices)?;
                                native.engine.sample_rate = profile.sample_rate;
                                #[cfg(feature = "cpal")]
                                if let shoop_rt_host_native::DriverConfig::Cpal(config) =
                                    &mut native.driver
                                {
                                    config.sample_rate = Some(profile.sample_rate);
                                }
                                #[cfg(feature = "cpal")]
                                if exact_devices {
                                    if let (
                                        AudioDriverConfig::Cpal(expected),
                                        shoop_rt_host_native::DriverConfig::Cpal(actual),
                                    ) = (&profile.configured, &native.driver)
                                    {
                                        use shoop_rt_host_native::DeviceSelection;
                                        ensure!(
                                            actual.output_device
                                                == DeviceSelection::Named(
                                                    expected
                                                        .output_device
                                                        .strip_prefix('=')
                                                        .unwrap_or(&expected.output_device)
                                                        .to_owned()
                                                )
                                                && actual.input_device
                                                    == if expected.input_device == "none" {
                                                        DeviceSelection::None
                                                    } else {
                                                        DeviceSelection::Named(
                                                            expected
                                                                .input_device
                                                                .strip_prefix('=')
                                                                .unwrap_or(&expected.input_device)
                                                                .to_owned(),
                                                        )
                                                    },
                                            "the confirmed CPAL device identity is unavailable"
                                        );
                                    }
                                }
                                #[cfg(not(feature = "cpal"))]
                                let _ = exact_devices;
                                let started = shoop_rt_host_native::start(&native)?;
                                ensure!(
                                    !exact_devices || started.host.warnings().is_empty(),
                                    "the target audio configuration could not be honored: {}",
                                    started.host.warnings().join("; ")
                                );
                                let status = started.host.status().driver;
                                let profile = ResolvedAudioDriverConfig {
                                    configured: profile.configured,
                                    sample_rate: status.sample_rate,
                                    buffer_size: status.buffer_frames,
                                    instance_name: started.host.instance_name().to_owned(),
                                };
                                let graph = crate::host_graph::controller(&started.host);
                                host = Some(started.host);
                                let mut backend =
                                    RtBackend::connect(started.client, Sleep::default())?;
                                backend.set_driver_profile(profile.clone());
                                Ok(DriverReply::Started {
                                    backend: Box::new(backend),
                                    graph,
                                    profile,
                                })
                            })(),
                        }
                        .map_err(|e: anyhow::Error| format!("{e:#}"));
                        if results.send(DriverCompletion { ticket, result }).is_err() {
                            break;
                        }
                    }
                }
            }
            let _ = stop(&mut host);
        })?;
    Ok((
        Owner {
            sender: sender.clone(),
            join: Some(join),
            stopping: None,
            stop_result: None,
        },
        Box::new(Controller {
            sender,
            receiver,
            ticket: None,
        }),
    ))
}

fn stop(host: &mut Option<NativeHost>) -> Result<()> {
    if let Some(host) = host.take() {
        let report = host.stop();
        ensure!(report.midi_reset, "MIDI reset was not confirmed");
        ensure!(
            report.phase != shoop_rt_host_native::RenderPhase::Faulted,
            "rendering faulted during cleanup"
        );
        ensure!(
            report.stats.midi_out_failed == 0,
            "MIDI output cleanup failed"
        );
        report
            .stopped
            .map_err(|e| anyhow!("engine cleanup failed: {e}"))?;
    }
    Ok(())
}

fn validate_target_processors(
    image: &shoop_app_protocol::persistence_image::Image,
    target: ConversionTarget,
    settings: &HostSettings,
) -> Result<()> {
    let effective = settings.processors.resolve_for_channels(
        shoop_rt_host_native::processor_support(),
        target.sample_rate,
        u8::try_from(settings.pools.channels)
            .map_err(|_| anyhow!("invalid processor channel capacity"))?,
    )?;
    let shoop_app_protocol::persistence_image::Manifest::Session(session) = &image.manifest else {
        return Err(anyhow!("driver preparation requires a session"));
    };
    let mut fx = 0;
    let mut synth = 0;
    for track in &session.tracks {
        if let TrackTopology::DryWet { processor_type, .. } = &track.topology {
            match processor_type.as_str() {
                TrackProcessorTypeId::BUILTIN_FX => fx += 1,
                TrackProcessorTypeId::OXISYNTH => synth += 1,
                _ => {}
            }
        }
    }
    ensure!(
        fx <= usize::from(effective.fx_instances)
            && synth <= usize::from(effective.synth_instances),
        "the target format cannot preserve the session's hosted processors"
    );
    Ok(())
}

fn available(kind: AudioDriverKind) -> bool {
    match kind {
        AudioDriverKind::Dummy => true,
        AudioDriverKind::Cpal => cfg!(feature = "cpal"),
        AudioDriverKind::Jack => cfg!(feature = "jack"),
        AudioDriverKind::WebAudio => false,
    }
}

fn discover(config: &AudioDriverConfig) -> Result<std::sync::Arc<[AudioDriverDescriptor]>> {
    #[allow(unused_mut)]
    let mut catalog = [
        AudioDriverKind::Dummy,
        AudioDriverKind::Cpal,
        AudioDriverKind::Jack,
    ]
    .map(|kind| AudioDriverDescriptor {
        kind,
        available: available(kind),
        unavailable_reason: (!available(kind))
            .then(|| "this driver is not built into this host".into()),
        ..Default::default()
    });
    #[cfg(feature = "cpal")]
    {
        let selector = match config {
            AudioDriverConfig::Cpal(config)
                if config.host != "default" && !config.host.is_empty() =>
            {
                Some(config.host.as_str())
            }
            _ => None,
        };
        catalog[1].hosts = shoop_rt_host_native::cpal_hosts();
        match shoop_rt_host_native::discover(selector) {
            Ok(inventory) => {
                catalog[1].input_devices = inventory
                    .inputs
                    .into_iter()
                    .map(|d| d.name)
                    .filter(|name| name.len() <= 128 && !name.contains('\0'))
                    .take(128)
                    .collect();
                catalog[1].output_devices = inventory
                    .outputs
                    .into_iter()
                    .map(|d| d.name)
                    .filter(|name| name.len() <= 128 && !name.contains('\0'))
                    .take(128)
                    .collect();
                catalog[1].midi_inputs = inventory
                    .midi_inputs
                    .into_iter()
                    .filter(|name| name.len() <= 128 && !name.contains('\0'))
                    .take(128)
                    .collect();
                catalog[1].midi_outputs = inventory
                    .midi_outputs
                    .into_iter()
                    .filter(|name| name.len() <= 128 && !name.contains('\0'))
                    .take(128)
                    .collect();
            }
            Err(error) => catalog[1].unavailable_reason = Some(error.to_string()),
        }
    }
    #[cfg(not(feature = "cpal"))]
    let _ = config;
    Ok(std::sync::Arc::from(catalog))
}

fn strict_config(
    config: &AudioDriverConfig,
    settings: &HostSettings,
    enforce_request_bounds: bool,
) -> Result<NativeConfig> {
    ensure!(
        (!enforce_request_bounds || config.bounded()) && available(config.kind()),
        "requested native driver is unavailable or invalid"
    );
    let inventory = crate::engine::inventory(config);
    let (mut native, warnings) = crate::engine::native_config(config, &settings.pools, &inventory);
    ensure!(
        warnings.is_empty(),
        "requested audio profile cannot be honored: {}",
        warnings.join("; ")
    );
    native.engine.processors = settings.processors;
    Ok(native)
}

fn preflight(
    config: &AudioDriverConfig,
    settings: &HostSettings,
) -> Result<ResolvedAudioDriverConfig> {
    let mut native = strict_config(config, settings, true)?;
    #[allow(unused_mut)]
    let mut configured = config.clone();
    #[cfg(feature = "cpal")]
    if let (shoop_rt_host_native::DriverConfig::Cpal(native_cpal), AudioDriverConfig::Cpal(cpal)) =
        (&mut native.driver, &mut configured)
    {
        *native_cpal = shoop_rt_host_native::resolve_cpal_devices(native_cpal)?;
        cpal.host = native_cpal.host.clone().unwrap();
        cpal.output_device = crate::engine::resolved_device_selector(&native_cpal.output_device);
        cpal.input_device = crate::engine::resolved_device_selector(&native_cpal.input_device);
        cpal.midi_inputs = crate::engine::resolved_midi_selectors(&native_cpal.midi_inputs);
        cpal.midi_outputs = crate::engine::resolved_midi_selectors(&native_cpal.midi_outputs);
        ensure!(
            configured.bounded(),
            "resolved device or MIDI names exceed the audio profile limits"
        );
    }
    let (sample_rate, buffer_size, instance_name) = match &native.driver {
        shoop_rt_host_native::DriverConfig::Dummy(config) => (
            config.sample_rate,
            config.buffer_frames as u32,
            "Dummy".to_owned(),
        ),
        #[cfg(feature = "cpal")]
        shoop_rt_host_native::DriverConfig::Cpal(config) => {
            let (rate, buffer) =
                shoop_rt_host_native::preflight_cpal(config, native.engine.max_frames)?;
            (rate, buffer, config.client_name.clone())
        }
        #[cfg(feature = "jack")]
        shoop_rt_host_native::DriverConfig::Jack(config) => {
            let (client, _) =
                jack::Client::new(&config.client_name, jack::ClientOptions::NO_START_SERVER)?;
            (
                client.sample_rate(),
                client.buffer_size(),
                client.name().to_owned(),
            )
        }
    };
    native.engine.sample_rate = sample_rate;
    shoop_rt_host_native::validate_processor_capacity(&native.engine)?;
    Ok(ResolvedAudioDriverConfig {
        configured,
        sample_rate,
        buffer_size,
        instance_name,
    })
}
