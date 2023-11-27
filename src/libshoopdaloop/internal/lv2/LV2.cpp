#include "LV2.h"
#include "ProcessProfiling.h"
#include "types.h"
#include <lilv_wrappers.h>

#ifdef _WIN32
#include "Windows.h"
#include <iostream>
#endif

namespace {
    std::atomic<bool> g_initialized = false;
}

LV2::LV2() : m_world(nullptr) {}

LV2::~LV2() { 
    if (m_world) {
        lilv_world_free(m_world);
    }
}

template <typename TimeType, typename SizeType>
std::shared_ptr<CarlaLV2ProcessingChain<TimeType, SizeType>>
LV2::create_carla_chain(fx_chain_type_t type, uint32_t sample_rate,
                        std::string title,
                        std::shared_ptr<profiling::Profiler> maybe_profiler) {
    if (!m_world) {
        if (!::g_initialized) {
            initialize_lilv(1);
        #ifdef _WIN32
            // Ensure we have default LV2 path set up
            auto current = getenv("LV2_PATH");
            std::string newval = (current ? std::string(current) : "") + ";C:\\Program Files\\Common Files\\LV2";
            SetEnvironmentVariable("LV2_PATH", newval.c_str());
        #endif
            ::g_initialized = true;
        }

        m_world = lilv_world_new();
        lilv_world_load_all(m_world);
    }

    return std::make_shared<CarlaLV2ProcessingChain<TimeType, SizeType>>(
        m_world, type, sample_rate, title, maybe_profiler);
}

template std::shared_ptr<CarlaLV2ProcessingChain<uint32_t, uint16_t>>
LV2::create_carla_chain(
    fx_chain_type_t type, uint32_t sample_rate, std::string title,
    std::shared_ptr<profiling::Profiler> maybe_profiler);
template std::shared_ptr<CarlaLV2ProcessingChain<uint32_t, uint32_t>>
LV2::create_carla_chain(
    fx_chain_type_t type, uint32_t sample_rate, std::string title,
    std::shared_ptr<profiling::Profiler> maybe_profiler);
template std::shared_ptr<CarlaLV2ProcessingChain<uint16_t, uint16_t>>
LV2::create_carla_chain(
    fx_chain_type_t type, uint32_t sample_rate, std::string title,
    std::shared_ptr<profiling::Profiler> maybe_profiler);
template std::shared_ptr<CarlaLV2ProcessingChain<uint16_t, uint32_t>>
LV2::create_carla_chain(
    fx_chain_type_t type, uint32_t sample_rate, std::string title,
    std::shared_ptr<profiling::Profiler> maybe_profiler);