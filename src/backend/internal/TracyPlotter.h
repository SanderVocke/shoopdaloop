#pragma once

#include "TracingRegistry.h"
#include <cstdint>
#include <string>
#include <string_view>

/// RAII helper for plotting values to Tracy from C++.
///
/// The Rust executable owns the tracy-client singleton and registers
/// function-pointer callbacks at startup.  This class calls through
/// them via TracingRegistry.  When callbacks are not registered (or
/// tracing is disabled) every operation is a no-op with negligible
/// overhead (one pointer load + one atomic load).
///
/// This class supports dynamic base identifiers, similar to the Rust
/// TracyPlotter. The plot name is constructed as "{base_identifier}/{suffix}".
/// If the base_identifier changes, a new plot name is registered.
///
/// Usage:
///   // During initialisation (not on audio thread):
///   TracyPlotter latency_plotter{"input_peak"};  // suffix only
///
///   // On audio thread (zero overhead when tracing disabled):
///   latency_plotter.plot(computed_value, "port_1");  // full name: "port_1/input_peak"
class TracyPlotter {
    std::string m_suffix;
    mutable std::string m_last_base_identifier;
    mutable uint32_t m_plot_id{0};

public:
    /// Create a plotter with a suffix.
    ///
    /// The full plot name will be "{base_identifier}/{suffix}" when plot() is called.
    /// Call this during initialisation, NOT on hot paths.
    explicit TracyPlotter(std::string_view suffix)
        : m_suffix(suffix)
    {}

    /// Plot a value to Tracy with the given base identifier.
    ///
    /// If the base_identifier changes from the last call, a new plot name
    /// is registered (and leaked). This is safe to call frequently - the
    /// overhead is minimal when the identifier doesn't change.
    /// Returns immediately when tracing is disabled.
    inline void plot(double value, std::string_view base_identifier) {
        if (!TracingRegistry::is_enabled()) {
            return;
        }
        
        // Check if we need to re-register the plot name
        if (m_plot_id == 0 || m_last_base_identifier != base_identifier) {
            // Construct full name
            std::string full_name;
            if (m_suffix.empty()) {
                full_name = std::string(base_identifier);
            } else {
                full_name = std::string(base_identifier) + "/" + m_suffix;
            }
            
            // Register and cache
            m_plot_id = TracingRegistry::register_plot_name(
                full_name.data(), full_name.size());
            m_last_base_identifier = std::string(base_identifier);
        }
        
        TracingRegistry::plot(m_plot_id, value);
    }

    /// Legacy method for static plot names (without dynamic base identifier).
    /// Use this for plots that don't need object-specific naming.
    inline void plot(double value) const {
        if (m_plot_id == 0) {
            // This plotter was never configured with a base identifier
            // and doesn't have a pre-registered ID. No-op.
            return;
        }
        TracingRegistry::plot(m_plot_id, value);
    }

    /// Check if Tracy tracing is currently active.
    static inline bool is_enabled() {
        return TracingRegistry::is_enabled();
    }
};