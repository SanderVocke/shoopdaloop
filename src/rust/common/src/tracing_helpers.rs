/// Helper to efficiently plot values to Tracy, handling plot name caching and leakage.
/// Zero-sized when tracing is disabled.
pub struct TracyPlotter {
    #[cfg(feature = "tracing")]
    suffix: String,
    #[cfg(feature = "tracing")]
    last_base_identifier: Option<String>,
    #[cfg(feature = "tracing")]
    plot_name: Option<tracy_client::PlotName>,
}

impl Default for TracyPlotter {
    fn default() -> Self {
        #[cfg(feature = "tracing")]
        {
            Self::new("")
        }
        #[cfg(not(feature = "tracing"))]
        {
            Self {}
        }
    }
}

impl TracyPlotter {
    pub fn new(suffix: impl Into<String>) -> Self {
         #[cfg(feature = "tracing")]
        {
            Self {
                suffix: suffix.into(),
                last_base_identifier: None,
                plot_name: None,
            }
        }
        #[cfg(not(feature = "tracing"))]
        {
            let _ = suffix;
            Self {}
        }
    }

    /// Plots a value to Tracy.
    /// 
    /// If `base_identifier` changes, a new `PlotName` is created (and leaked).
    /// Safe to call frequently.
    pub fn plot(&mut self, value: f64, base_identifier: &str) {
        #[cfg(feature = "tracing")]
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
        #[cfg(not(feature = "tracing"))]
        {
            let _ = value;
            let _ = base_identifier;
        }
    }
}
