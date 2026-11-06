use alloc::{boxed::Box, string::String, sync::Arc};
use shoop_app_api::{
    AudioDriverConfig, AudioDriverDescriptor, ConversionTarget, HostResourceLimits,
    ResolvedAudioDriverConfig,
};
use shoop_app_protocol::persistence_image::Image;
use shoop_backend::Backend;

pub enum DriverWork {
    Discover(AudioDriverConfig),
    Preflight(AudioDriverConfig),
    Convert {
        image: Arc<Image>,
        target: ConversionTarget,
        limits: HostResourceLimits,
    },
    Start(ResolvedAudioDriverConfig),
    Persist(AudioDriverConfig),
}

pub enum DriverReply {
    Discovered(Arc<[AudioDriverDescriptor]>),
    Resolved(ResolvedAudioDriverConfig),
    Converted(Arc<Image>),
    Started {
        backend: Box<dyn Backend + Send>,
        graph: Option<Box<dyn crate::HostGraphController>>,
        profile: ResolvedAudioDriverConfig,
    },
    Persisted,
}

pub struct DriverCompletion {
    pub ticket: u64,
    pub result: Result<DriverReply, String>,
}

pub trait AudioDriverController: Send {
    fn submit(&mut self, ticket: u64, work: DriverWork) -> Result<(), String>;
    fn poll(&mut self) -> Option<DriverCompletion>;
}
