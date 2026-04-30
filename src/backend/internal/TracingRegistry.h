#pragma once

#include <cstdint>

/// Function-pointer callbacks provided by the Rust executable at runtime.
///
/// The Rust side owns the tracy-client singleton and registers these
/// callbacks during initialisation.  If tracing is disabled or the
/// callbacks have not been registered every operation is a no-op.
struct TracingCallbacks {
    bool (*is_tracing_enabled)();
    uint32_t (*register_plot_name)(const char* name, std::size_t len);
    void (*plot_by_id)(uint32_t id, double value);
};

/// Global tracing registry living inside shoopdaloop_backend.so.
///
/// Usage:
///   // During init (called once from Rust):
///   TracingRegistry::register(&callbacks);
///
///   // On hot paths:
///   if (TracingRegistry::is_enabled()) {
///       TracingRegistry::plot(id, value);
///   }
class TracingRegistry {
    static inline const TracingCallbacks* s_callbacks{nullptr};

public:
    /// Register the callbacks.  Called once at startup from the Rust side.
    /// Passing nullptr unregisters (restores the no-op state).
    static void register_callbacks(const TracingCallbacks* cb) {
        s_callbacks = cb;
    }

    /// Returns true if callbacks are registered AND tracing is active.
    static inline bool is_enabled() {
        return s_callbacks && s_callbacks->is_tracing_enabled();
    }

    /// Register a plot name and return a numeric ID.
    /// If callbacks are not registered returns 0 (a valid but unused ID).
    static inline uint32_t register_plot_name(const char* name, std::size_t len) {
        if (s_callbacks) {
            return s_callbacks->register_plot_name(name, len);
        }
        return 0;
    }

    /// Plot a value by ID.  No-op when callbacks are not registered.
    static inline void plot(uint32_t id, double value) {
        if (s_callbacks && s_callbacks->is_tracing_enabled()) {
            s_callbacks->plot_by_id(id, value);
        }
    }
};
