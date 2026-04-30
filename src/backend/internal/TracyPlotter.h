#pragma once

#include "TracingRegistry.h"
#include <cstdint>
#include <string_view>

/// RAII helper for plotting values to Tracy from C++.
///
/// The Rust executable owns the tracy-client singleton and registers
/// function-pointer callbacks at startup.  This class calls through
/// them via TracingRegistry.  When callbacks are not registered (or
/// tracing is disabled) every operation is a no-op with negligible
/// overhead (one pointer load + one atomic load).
///
/// Usage:
///   // During initialisation (not on audio thread):
///   TracyPlotter latency_plotter{"audio/thread_latency_ms"};
///
///   // On audio thread (zero overhead when tracing disabled):
///   latency_plotter.plot(computed_latency);
class TracyPlotter {
    uint32_t m_plot_id;

public:
    /// Register a plot name with Tracy and store the returned ID.
    ///
    /// Call this during initialisation, NOT on hot paths.  The name
    /// is allocated once and lives for the program's lifetime.
    explicit TracyPlotter(std::string_view name)
        : m_plot_id(TracingRegistry::register_plot_name(
              name.data(), name.size()))
    {}

    /// Plot a value to Tracy.  Returns immediately when tracing is disabled.
    inline void plot(double value) const {
        TracingRegistry::plot(m_plot_id, value);
    }

    /// Check if Tracy tracing is currently active.
    static inline bool is_enabled() {
        return TracingRegistry::is_enabled();
    }
};
