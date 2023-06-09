
#include <string_view>
#include <memory>
#include <type_traits>
#include <vector>
#include <map>
#include <mutex>

// This module allows profiling the audio process thread.
// Different software modules can register to log profiling data.
// Data is then tracked and averaged over time.
// Periodically, a report is generated which can be received by subscribers.

// The profiling item keys are hiearchical. That is to say:
// Process.Audio.Record will be regarded as a subset of Process.Audio.

namespace profiling {

struct ProfilingItem;

struct ProfilingReportItem {
    std::string key;
    size_t n_samples;
    float avg;
    float worst;
    float most_recent;
};

using ProfilingReport = std::vector<ProfilingReportItem>;

class Profiler {
    std::recursive_mutex m_registry_access;
    std::map<std::string, std::weak_ptr<ProfilingItem>> m_registry;
    
public:
    std::shared_ptr<ProfilingItem> get_profiling_item(std::string name);
    void spent(std::shared_ptr<ProfilingItem> &item, float time);
    ProfilingReport report(); 
};

}