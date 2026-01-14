use std::sync::atomic::{AtomicBool, Ordering};

static TRACING_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn set_tracing_enabled(enabled: bool) {
    TRACING_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn is_tracing_enabled() -> bool {
    TRACING_ENABLED.load(Ordering::Relaxed)
}

/// Helper to efficiently plot values to Tracy, handling plot name caching and leakage.
pub struct TracyPlotter {
    suffix: String,
    last_base_identifier: Option<String>,
    plot_name: Option<tracy_client::PlotName>,
}

impl Default for TracyPlotter {
    fn default() -> Self {
        Self::new("")
    }
}

impl TracyPlotter {
    pub fn new(suffix: impl Into<String>) -> Self {
        Self {
            suffix: suffix.into(),
            last_base_identifier: None,
            plot_name: None,
        }
    }

    /// Plots a value to Tracy.
    /// 
    /// If `base_identifier` changes, a new `PlotName` is created (and leaked).
    /// Safe to call frequently.
    pub fn plot(&mut self, value: f64, base_identifier: &str) {
        if !is_tracing_enabled() {
            return;
        }
        if let Some(client) = tracy_client::Client::running() {
            if self.plot_name.is_none() || self.last_base_identifier.as_deref() != Some(base_identifier) {
                let full_name = if self.suffix.is_empty() {
                    base_identifier.to_string()
                } else {
                    format!("{}/{}", base_identifier, self.suffix)
                };
                self.plot_name = Some(tracy_client::PlotName::new_leak(full_name));
                self.last_base_identifier = Some(base_identifier.to_string());
            }

            if let Some(plot_name) = &self.plot_name {
                client.plot(plot_name.clone(), value);
            }
        }
    }
}
