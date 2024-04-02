#include "ProcessProfiling.h"
#include <mutex>
#include <optional>
#include <atomic>
#include <functional>
#include <map>

namespace profiling {

struct ProfilingItemPrivate {
    std::atomic<float> n_reported = 0.0f;
    std::atomic<float> summed = 0.0f;
    std::atomic<float> most_recent = -1.0f;
    std::atomic<float> worst = -1.0f;
    std::atomic<float> current_iteration = 0.0f;

    void reset(std::function<void(float, float, float, float)> report_cb) {
        auto _n_reported = n_reported.load();
        auto _summed = summed.load();
        auto _most_recent = most_recent.load();
        auto _worst = worst.load();

        current_iteration = 0.0f;
        n_reported = 0.0f;
        summed = 0.0f;
        most_recent = -1.0f;
        worst = -1.0f;

        if(report_cb) {
            report_cb(_n_reported,
                      _n_reported >= 1.0f ? _summed / _n_reported : -1.0f,
                      _worst,
                      _most_recent);
        }
    }

    void log_time(float t) {
        current_iteration.fetch_add(t);
    }

    void next_iteration() {
        auto _current_iteration = current_iteration.load();
        current_iteration = 0.0f;

        n_reported.fetch_add(1.0f);
        most_recent = _current_iteration;
        if(worst < _current_iteration) {
            worst = _current_iteration;
        }
        summed.fetch_add(_current_iteration);
    }
};

struct ProfilerPrivate {
    std::recursive_mutex m_registry_access;
    std::map<std::string, std::weak_ptr<ProfilingItem>> m_registry;
};

void Profiler::next_iteration() {
    if (!g_ProfilingEnabled) { return; }

    std::lock_guard<std::recursive_mutex> g(pvt->m_registry_access);

    for (auto &item : pvt->m_registry) {
        if (auto l = item.second.lock()) {
            l->next_iteration();
        }
    }
}

ProfilingItem::ProfilingItem() {
    pvt = std::make_unique<ProfilingItemPrivate>();
}

void ProfilingItem::log_time(float time) {
    if (!g_ProfilingEnabled) { return; }

    pvt->log_time(time);
}

void ProfilingItem::reset(std::function<void(float, float, float, float)> report_cb) {
    if (!g_ProfilingEnabled) { return; }

    pvt->reset(report_cb);
}

void ProfilingItem::next_iteration() {
    if (!g_ProfilingEnabled) { return; }

    pvt->next_iteration();
}

ProfilingReport Profiler::report() {
    if (!g_ProfilingEnabled) { return ProfilingReport{}; }

    std::lock_guard<std::recursive_mutex> g(pvt->m_registry_access);

    ProfilingReport rval;
    rval.reserve(pvt->m_registry.size());
    for(auto &item : pvt->m_registry) {
        if (auto i = item.second.lock()) {
            auto &key = item.first;
            rval.push_back(ProfilingReportItem {});
            auto &report_item = rval.back();
            report_item.key = key;
            i->reset([&](uint32_t n, float avg, float worst, float last) {
                report_item.n_samples = n;
                report_item.avg = avg;
                report_item.worst = worst;
                report_item.most_recent = last;
            });
        }
    }

    return rval;
}

std::shared_ptr<ProfilingItem> Profiler::maybe_get_profiling_item(std::string name) {
    if (!g_ProfilingEnabled) { return nullptr; }

    std::lock_guard<std::recursive_mutex> g(pvt->m_registry_access);

    auto maybe_r = pvt->m_registry.find(name);
    if (maybe_r != pvt->m_registry.end()) {
        if (auto maybe_rr = maybe_r->second.lock()) {
            return maybe_rr;
        }
    }

    auto rr = new ProfilingItem;
    auto r = std::shared_ptr<ProfilingItem>(rr);
    pvt->m_registry[name] = r;
    return r;
}

Profiler::Profiler() {
    pvt = std::make_unique<ProfilerPrivate>();
}

Profiler::~Profiler() {
    std::lock_guard<std::recursive_mutex> g(pvt->m_registry_access);

    pvt->m_registry.clear();
}

}