#pragma once
#include <memory>
#include <vector>
#include <chrono>
#include <functional>
#include <string>
#include "shoop_shared_ptr.h"

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
    std::string key = "";
    float n_samples = 0.0f;
    float avg = 0.0f;
    float worst = 0.0f;
    float most_recent = 0.0f;
};

using ProfilingReport = std::vector<ProfilingReportItem>;

struct ProfilerPrivate;
class Profiler {
    std::unique_ptr<ProfilerPrivate> pvt;
    
public:
    shoop_shared_ptr<ProfilingItem> maybe_get_profiling_item(std::string name);
    ProfilingReport report();
    void next_iteration();

    Profiler();
    ~Profiler();
};


namespace {

inline void log_time(float time, shoop_shared_ptr<ProfilingItem> item) {
    if(item) { item->log_time(time); }
}
 
template<typename ProfItem, typename... ProfItems>
void log_time(float time, ProfItem first, ProfItems... rest)
{
    log_time(time, first);
    log_time(time, rest...);
}

inline bool any_valid(shoop_shared_ptr<ProfilingItem> item) {
    return (bool) item;
}
 
template<typename ProfItem, typename... ProfItems>
bool any_valid(ProfItem first, ProfItems... rest)
{
    return any_valid(first) || any_valid(rest...);
}

}

template<typename ProfItem, typename... ProfItems>
inline void stopwatch(std::function<void()> fn, ProfItem first, ProfItems... rest) {
    using namespace std::chrono;
    if(g_ProfilingEnabled && any_valid(first, rest...)) {
        auto start = high_resolution_clock::now();
        fn();
        auto end = high_resolution_clock::now();
        float us = duration_cast<microseconds>(end - start).count();
        log_time(us, first, rest...);
    } else {
        fn();
    }
}

}