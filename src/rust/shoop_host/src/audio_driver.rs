use anyhow::{anyhow, ensure, Result};
use shoop_app::{AudioDriverController, DriverCompletion, DriverReply, DriverWork};
use shoop_app_api::*;
use shoop_rt_backend::{RtBackend, Sleep};
use shoop_rt_host_native::{NativeConfig, NativeHost};
use std::{
    sync::mpsc::{self, Receiver, SyncSender},
    thread::{self, JoinHandle},
    time::Duration,
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
}
impl Owner {
    pub fn stop(&mut self) -> Result<(), String> {
        if self.join.is_none() {
            return Ok(());
        }
        let (tx, rx) = mpsc::channel();
        self.sender
            .try_send(Request::Stop(tx))
            .map_err(|_| "audio lifecycle worker is busy or unavailable".to_owned())?;
        let result = rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "audio lifecycle cleanup did not finish".to_owned())?;
        if let Some(join) = self.join.take() {
            join.join()
                .map_err(|_| "audio lifecycle worker panicked".to_owned())?;
        }
        result
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
                                conversion::convert(image, target, limits)
                                    .map(DriverReply::Converted)
                            }
                            DriverWork::Persist(config) => {
                                preferences(&config).map(|()| DriverReply::Persisted)
                            }
                            DriverWork::Start(profile) => (|| {
                                stop(&mut host)?;
                                let native = strict_config(&profile.configured, &settings)?;
                                let started = shoop_rt_host_native::start(&native)?;
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

fn strict_config(config: &AudioDriverConfig, settings: &HostSettings) -> Result<NativeConfig> {
    ensure!(
        config.bounded() && available(config.kind()),
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
    let mut native = strict_config(config, settings)?;
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
        configured: config.clone(),
        sample_rate,
        buffer_size,
        instance_name,
    })
}
