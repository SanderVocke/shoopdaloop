#pragma once

#include "tracing_cxx_bridge/src/cxx.rs.h"
#include <cstdint>
#include <string_view>

/// RAII helper for plotting values to Tracy from C++.
///
/// Mirrors the Rust `TracyPlotter` pattern: register once during
/// initialization, then plot by ID on hot paths with minimal overhead.
///
/// Usage:
///   // During initialization (not on audio thread):
///   TracyPlotter latency_plotter{"audio/thread_latency_ms"};
///
///   // On audio thread (zero overhead when tracing disabled):
///   latency_plotter.plot(computed_latency);
class TracyPlotter {
    uint32_t m_plot_id;

public:
    /// Register a plot name with Tracy and store the returned ID.
    ///
    /// Call this during initialization, NOT on hot paths. The name
    /// is allocated once and lives for the program's lifetime.
    explicit TracyPlotter(std::string_view name)
        : m_plot_id(shoop_tracing::register_plot_name(
              rust::Str(name.data(), name.size())))
    {}

    /// Plot a value to Tracy. Returns immediately when tracing is disabled.
    ///
    /// Safe to call from audio threads. The overhead when enabled is:
    ///   1 atomic load + mutex lock + Vec::get + clone + Tracy call.
    /// When disabled (the common case), only the atomic load executes.
    inline void plot(double value) const {
        if (shoop_tracing::is_tracing_enabled()) {
            shoop_tracing::plot_by_id(m_plot_id, value);
        }
    }

    /// Check if Tracy tracing is currently active.
    ///
    /// Compiles to a single atomic load. Branch predictor learns the
    /// false path when tracing is disabled, making this essentially
    /// zero overhead.
    static inline bool is_enabled() {
        return shoop_tracing::is_tracing_enabled();
    }
};
