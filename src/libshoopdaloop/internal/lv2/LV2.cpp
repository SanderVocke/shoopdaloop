#define IMPLEMENT_LV2_H
#include "LV2.h"
#include "ProcessProfiling.h"
#include "types.h"
#include <lilv/lilv.h>

LV2::LV2() {
    m_world = lilv_world_new();
    lilv_world_load_all(m_world);
}

LV2::~LV2() { lilv_world_free(m_world); }

template <typename TimeType, typename SizeType>
std::shared_ptr<CarlaLV2ProcessingChain<TimeType, SizeType>>
LV2::create_carla_chain(fx_chain_type_t type, size_t sample_rate,
                        std::string title,
                        std::shared_ptr<profiling::Profiler> maybe_profiler) {
    return std::make_shared<CarlaLV2ProcessingChain<TimeType, SizeType>>(
        m_world, type, sample_rate, title, maybe_profiler);
}

template std::shared_ptr<CarlaLV2ProcessingChain<uint32_t, uint16_t>>
LV2::create_carla_chain(
    fx_chain_type_t type, size_t sample_rate, std::string title,
    std::shared_ptr<profiling::Profiler> maybe_profiler = nullptr);
template std::shared_ptr<CarlaLV2ProcessingChain<uint32_t, uint32_t>>
LV2::create_carla_chain(
    fx_chain_type_t type, size_t sample_rate, std::string title,
    std::shared_ptr<profiling::Profiler> maybe_profiler = nullptr);
template std::shared_ptr<CarlaLV2ProcessingChain<uint16_t, uint16_t>>
LV2::create_carla_chain(
    fx_chain_type_t type, size_t sample_rate, std::string title,
    std::shared_ptr<profiling::Profiler> maybe_profiler = nullptr);
template std::shared_ptr<CarlaLV2ProcessingChain<uint16_t, uint32_t>>
LV2::create_carla_chain(
    fx_chain_type_t type, size_t sample_rate, std::string title,
    std::shared_ptr<profiling::Profiler> maybe_profiler = nullptr);