//! Serialized Marlin composition with the shared native drivers.
use anyhow::Result;
use shoop_audio_drivers as drivers;
pub use shoop_rt_host_marlin::Endpoint;
use shoop_rt_transport::{
    carrier::{InProcess, Outbox},
    client::ProtocolClient,
};
use std::{any::Any, sync::Arc};

pub use crate::fpga_reference::Driver;
pub struct Started {
    pub client: ProtocolClient<InProcess<Endpoint>>,
    pub driver: Driver,
}
pub fn start(quantum: u32, sample_rate: u32) -> Result<Started> {
    start_with_memory(quantum, sample_rate, 16)
}
pub fn start_with_memory(quantum: u32, sample_rate: u32, memory_mb: u32) -> Result<Started> {
    start_driver(
        drivers::DriverConfig::Dummy(drivers::DummyConfig {
            sample_rate,
            buffer_frames: quantum,
            ..Default::default()
        }),
        quantum,
        memory_mb,
    )
}
pub fn template(quantum: u32, memory_mb: u32, max_frames: usize) -> drivers::Template {
    drivers::Template {
        max_frames,
        jack_capacities: [1, 2, 0, 0],
        jack_fixed_ports: true,
        factory: Arc::new(Factory {
            quantum,
            memory_mb,
            max_frames,
        }),
    }
}
pub fn start_driver(
    driver: drivers::DriverConfig,
    quantum: u32,
    memory_mb: u32,
) -> Result<Started> {
    let max = driver.max_frames_hint().max(quantum as usize);
    let started = drivers::start(&drivers::DriverSettings {
        driver,
        engine: template(quantum, memory_mb, max),
    })?;
    let endpoint = *started
        .endpoint
        .downcast::<Endpoint>()
        .map_err(|_| anyhow::anyhow!("Marlin endpoint mismatch"))?;
    Ok(Started {
        client: ProtocolClient::new(InProcess::new(endpoint, started.outbox)),
        driver: Driver::new(started.host),
    })
}
struct Factory {
    max_frames: usize,
    quantum: u32,
    memory_mb: u32,
}
struct Observer(Arc<shoop_rt_host_reference::Status>);
impl drivers::Observer for Observer {
    fn driver(&self, d: shoop_rt_protocol::DriverStatus) {
        self.0.update_driver(d);
    }
    fn xrun(&self) {
        self.0.xrun();
    }
    fn load(&self, load: f32, _frames: u32, _sample_rate: u32, _now: u64) {
        self.0.load(load);
    }
    fn requires_reset(&self) -> bool {
        false
    }
}
struct Render(rt_runtime::host_adapter::QuantumAdapter<shoop_rt_host_marlin::Bridge>);
impl rt_runtime::host::Renderer for Render {
    fn next_frame(&self) -> rt_runtime::Frame {
        rt_runtime::host::Renderer::next_frame(&self.0)
    }
    fn process(
        &mut self,
        audio: rt_runtime::buffer::AudioBlock<'_, '_>,
        midi: rt_runtime::buffer::MidiBlock<'_>,
    ) -> Result<rt_runtime::host::Report, rt_runtime::Error> {
        rt_runtime::host::Renderer::process(&mut self.0, audio, midi)
    }
}
impl drivers::Processor for Render {
    fn finish(self: Box<Self>) -> Result<Box<dyn Any + Send>, rt_runtime::Error> {
        let mut bridge = self.0.into_inner();
        bridge.stop()?;
        Ok(Box::new(()))
    }
}
impl drivers::Factory for Factory {
    fn prepare(&self, opened: drivers::Opened) -> Result<drivers::Parts, drivers::StartError> {
        let prepared = shoop_rt_host_marlin::prepare(
            self.quantum,
            opened.driver,
            self.memory_mb,
            self.max_frames,
        )
        .map_err(|e| drivers::StartError::Factory(e.into()))?;
        Ok(drivers::Parts {
            buffers: drivers::BufferConfig {
                host: opened.ports,
                max_frames: self.max_frames,
                midi_events: 256,
                midi_bytes: 65536,
                output_midi_events: 256,
                output_midi_bytes: 65536,
            },
            renderer: Box::new(Render(prepared.renderer)),
            observer: Arc::new(Observer(prepared.status)),
            endpoint: Box::new(prepared.endpoint),
            outbox: Outbox::new(64, 8, 0),
            master_connections: vec![(0, 0), (1, 1)],
        })
    }
}

pub fn start_selected(
    driver: &shoop_app_api::AudioDriverConfig,
    pools: &shoop_app_api::NewEnginePools,
    input: Option<fn(usize, u64) -> f32>,
) -> Result<Started> {
    use shoop_app_api::AudioDriverConfig;
    anyhow::ensure!(
        matches!(driver, AudioDriverConfig::Dummy(_))
            || (cfg!(feature = "cpal") && matches!(driver, AudioDriverConfig::Cpal(_)))
            || (cfg!(feature = "jack") && matches!(driver, AudioDriverConfig::Jack(_))),
        "selected driver is unavailable for this native host build"
    );
    let (mut config, warnings) =
        crate::engine::native_config(driver, pools, &crate::engine::inventory(driver));
    for warning in warnings {
        tracing::warn!(%warning,"Marlin driver setup");
    }
    match &mut config.driver {
        drivers::DriverConfig::Dummy(dummy) => dummy.input = input,
        #[cfg(any(feature = "cpal", feature = "jack"))]
        _ => {}
    }
    start_driver(config.driver, pools.quantum, pools.loop_memory_mb)
}
