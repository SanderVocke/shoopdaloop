use clap::ValueEnum;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Polling {
    Yield,
    Spin,
}
impl Polling {
    pub fn idle(self) {
        std::hint::spin_loop();
        if matches!(self, Self::Yield) {
            std::thread::yield_now();
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, serde::Deserialize)]
pub struct ServiceGaps {
    pub maximum_ns: u64,
    pub over_100us: u64,
    pub over_1ms: u64,
    pub over_10ms: u64,
}
impl ServiceGaps {
    pub fn observe(&mut self, elapsed_ns: u64) {
        self.maximum_ns = self.maximum_ns.max(elapsed_ns);
        self.over_100us += u64::from(elapsed_ns >= 100_000);
        self.over_1ms += u64::from(elapsed_ns >= 1_000_000);
        self.over_10ms += u64::from(elapsed_ns >= 10_000_000);
    }
}
