#pragma once
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

class Profiler;
struct ProfilingItemPrivate;
class ProfilingItem {
    std::unique_ptr<ProfilingItemPrivate> pvt;

    // Methods only available to Profiler
    friend class Profiler;
    ProfilingItem();
    void next_iteration();
    void reset(std::function<void(float, float, float, float)> report_cb);

public:
    void log_time(float time);
};

struct ProfilingReportItem {
    std::string key;
    float n_samples;
    float avg;
    float worst;
    float most_recent;
};

using ProfilingReport = std::vector<ProfilingReportItem>;

struct ProfilerPrivate;
class Profiler {
    std::unique_ptr<ProfilerPrivate> pvt;
    
public:
    std::shared_ptr<ProfilingItem> maybe_get_profiling_item(std::string name);
    ProfilingReport report();
    void next_iteration();

    Profiler();
    ~Profiler();
};

inline void stopwatch(std::function<void()> fn, std::shared_ptr<ProfilingItem> &item) {
    using namespace std::chrono;
    if(g_ProfilingEnabled && item) {
        auto start = high_resolution_clock::now();
        fn();
        auto end = high_resolution_clock::now();
        float us = duration_cast<microseconds>(end - start).count();
        item->log_time(us);
    }
}

}