
#include <string_view>
#include <memory>
#include <type_traits>
#include <vector>
#include <map>
#include <mutex>
#include <chrono>
#include <functional>

// This module allows profiling the audio process thread.
// Different software modules can register to log profiling data.
// Data is then tracked and averaged over time.
// Periodically, a report is generated which can be received by subscribers.

// The profiling item keys are hiearchical. That is to say:
// Process.Audio.Record will be regarded as a subset of Process.Audio.

namespace profiling {

#ifdef SHOOP_PROFILING
constexpr bool g_ProfilingEnabled = true;
#else
constexpr bool g_ProfilingEnabled = false;
#endif

struct ProfilingItem;

struct ProfilingReportItem {
    std::string key;
    float n_samples;
    float avg;
    float worst;
    float most_recent;
};

using ProfilingReport = std::vector<ProfilingReportItem>;

class Profiler {
    std::recursive_mutex m_registry_access;
    std::map<std::string, std::weak_ptr<ProfilingItem>> m_registry;
    
public:
    std::shared_ptr<ProfilingItem> maybe_get_profiling_item(std::string name);
    void log_time(std::shared_ptr<ProfilingItem> &item, float time);
    void next_iteration();
    ProfilingReport report();

    Profiler() = default;
};

inline void stopwatch(std::function<void()> fn, Profiler &prof, std::shared_ptr<ProfilingItem> &item) {
    using namespace std::chrono;
    if(g_ProfilingEnabled) {
        auto start = high_resolution_clock::now();
        fn();
        auto end = high_resolution_clock::now();
        float us = duration_cast<microseconds>(end - start).count();
        prof.log_time(item, us);
    }
}

}