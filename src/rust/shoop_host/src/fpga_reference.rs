//! Reference endpoint composition with the shared native drivers.
use anyhow::Result;
use shoop_audio_drivers as drivers;
pub use shoop_rt_host_reference::{program_limits, Endpoint};
use shoop_rt_transport::{
    carrier::{InProcess, Outbox},
    client::ProtocolClient,
};
use std::{any::Any, sync::Arc};

pub struct Driver {
    host: Option<drivers::DriverHost>,
}
impl Driver {
    pub(crate) fn new(host: drivers::DriverHost) -> Self {
        Self { host: Some(host) }
    }
    pub fn host(&self) -> &drivers::DriverHost {
        self.host.as_ref().expect("running reference driver")
    }
    pub fn dummy(&self) -> Option<&drivers::DummyControl> {
        self.host().dummy()
    }
}
impl Drop for Driver {
    fn drop(&mut self) {
        if let Some(host) = self.host.take() {
            host.stop();
        }
    }
}
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
        factory: Arc::new(Factory { quantum, memory_mb }),
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
        .map_err(|_| anyhow::anyhow!("reference endpoint mismatch"))?;
    Ok(Started {
        client: ProtocolClient::new(InProcess::new(endpoint, started.outbox)),
        driver: Driver::new(started.host),
    })
}
struct Factory {
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
struct Render(rt_runtime::host_adapter::QuantumAdapter<shoop_rt_host_reference::ReferenceRenderer>);
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
        Ok(Box::new(()))
    }
}
impl drivers::Factory for Factory {
    fn prepare(&self, opened: drivers::Opened) -> Result<drivers::Parts, drivers::StartError> {
        let prepared =
            shoop_rt_host_reference::prepare(self.quantum, opened.driver, self.memory_mb)
                .map_err(|e| drivers::StartError::Factory(e.into()))?;
        Ok(drivers::Parts {
            buffers: drivers::BufferConfig {
                host: opened.ports,
                max_frames: opened.driver.buffer_frames.max(self.quantum) as usize,
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
        tracing::warn!(%warning,"reference driver setup");
    }
    match &mut config.driver {
        drivers::DriverConfig::Dummy(dummy) => dummy.input = input,
        #[cfg(any(feature = "cpal", feature = "jack"))]
        _ => {}
    }
    start_driver(config.driver, pools.quantum, pools.loop_memory_mb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_rt_protocol::{immediate::GraphLoop, snapshot::Header};
    use shoop_rt_protocol::{
        immediate::{Batch, Code, Command, Response},
        ClientMessage, EngineMessage,
    };
    use shoop_rt_transport::carrier::Endpoint as _;
    use shoop_wasm_test_support::shoop_test;
    use std::{
        thread,
        time::{Duration, Instant},
    };
    fn response(client: &mut ProtocolClient<InProcess<Endpoint>>) -> Response {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(EngineMessage::ImmediateResponse(r)) = client.poll() {
                return r;
            }
            assert!(Instant::now() < deadline, "activation response timed out");
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn graph(revision: u64, source: Option<u32>) -> Batch {
        Batch::new(
            revision,
            revision,
            &[
                Command::BeginGraph {
                    quantum: 16,
                    loops: 1,
                },
                Command::ConfigureLoop(GraphLoop {
                    slot: 0,
                    generation: 3,
                    source,
                    input: Some(0),
                    gain: 1.0,
                    output: true,
                    repeat_sync: false,
                }),
                Command::ActivateGraph,
            ],
        )
        .unwrap()
    }
    #[shoop_test]
    fn installation_updates_and_client_lifetime_do_not_drive_processing() {
        let Started {
            mut client,
            driver: _driver,
        } = start(16, 8000).unwrap();
        assert!(client
            .carrier_mut()
            .endpoint_mut()
            .prepare(graph(0, None))
            .is_err());
        client.connect().unwrap();
        assert!(matches!(client.poll(), Some(EngineMessage::Welcome { .. })));
        client
            .immediate(Batch::new(0, 0, &[Command::LoopGain(0, 0.5)]).unwrap())
            .unwrap();
        assert_eq!(response(&mut client).records()[0].code(), Code::CannotApply);
        client.immediate(graph(0, Some(1))).unwrap();
        assert_eq!(response(&mut client).header.code(), Code::CannotApply);
        client.immediate(graph(0, None)).unwrap();
        let first = response(&mut client);
        assert_eq!(first.header.code(), Code::Applied);
        assert_eq!(first.header.graph(), 1);
        client.immediate(graph(0, None)).unwrap();
        assert_eq!(response(&mut client).header.code(), Code::CannotApply);
        client.immediate(graph(1, None)).unwrap();
        let next = response(&mut client);
        assert_eq!(next.header.code(), Code::Applied);
        assert_eq!(next.header.graph(), 2);
        let frame = |c: &mut ProtocolClient<InProcess<Endpoint>>| {
            let mut bytes = vec![0; c.carrier_mut().endpoint().snapshot_capacity()];
            let (_, len) = c
                .carrier_mut()
                .endpoint()
                .read_snapshot(&mut bytes)
                .unwrap();
            Header::read(&bytes[..len]).unwrap().frame
        };
        let before = frame(&mut client);
        client
            .carrier_mut()
            .endpoint_mut()
            .try_deliver(ClientMessage::Goodbye, &mut Outbox::new(1, 0, 0))
            .unwrap();
        thread::sleep(Duration::from_millis(20));
        assert!(frame(&mut client) > before);
    }
}
