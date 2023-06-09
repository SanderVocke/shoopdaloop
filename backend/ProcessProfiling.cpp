#include "ProcessProfiling.h"
#include <mutex>
#include <optional>
#include <atomic>
#include <functional>
#include <map>

namespace profiling {

struct ProfilingItem {
    size_t n_reported = 0;

    float summed = 0.0f;
    std::optional<float> most_recent;
    std::optional<float> worst;

    std::recursive_mutex mutex;

    void reset(std::function<void(size_t, float, float, float)> report_cb) {
        std::lock_guard<std::recursive_mutex> g(mutex);

        if(report_cb) {
            report_cb(n_reported,
                      n_reported ? summed / n_reported : -1.0f,
                      worst.value_or(-1.0f),
                      most_recent.value_or(-1.0f));
        }

        n_reported = 0;
        summed = 0.0f;
        most_recent = std::nullopt;
        worst = std::nullopt;
    }

    void spent(float time) {
        std::lock_guard<std::recursive_mutex> g(mutex);

        n_reported++;
        most_recent = time;
        if(worst.value_or(0.0f) < time) {
            worst = time;
        }
        summed += time;
    }
};

void spent(std::shared_ptr<ProfilingItem> &item, float time) {
    item->spent(time);
}

ProfilingReport report() {
    std::lock_guard<std::recursive_mutex> g(g_registry_access);

    ProfilingReport rval;
    rval.reserve(g_registry.size());
    for(auto &item : g_registry) {
        if (auto i = item.second.lock()) {
            auto &key = item.first;
            rval.push_back(ProfilingReportItem {});
            auto &report_item = rval.back();
            report_item.key = key;
            i->reset([&](size_t n, float avg, float worst, float last) {
                report_item.n_samples = n;
                report_item.avg = avg;
                report_item.worst = worst;
                report_item.most_recent = last;
            });
        }
    }

    return rval;
}

std::shared_ptr<ProfilingItem> get_profiling_item(std::string name) {
    std::lock_guard<std::recursive_mutex> g(g_registry_access);

    auto maybe_r = g_registry.find(name);
    if (maybe_r != g_registry.end()) {
        if (auto maybe_rr = maybe_r->second.lock()) {
            return maybe_rr;
        }
    }

    auto r = std::make_shared<ProfilingItem>();
    g_registry[name] = r;
    return r;
}

}