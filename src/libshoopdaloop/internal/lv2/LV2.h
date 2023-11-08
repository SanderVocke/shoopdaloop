#pragma once
#include "CarlaLV2ProcessingChain.h"
#include <memory>

class LV2 {
    LilvWorld *m_world;

  public:
    LV2();
    ~LV2();

    template <typename TimeType, typename SizeType>
    std::shared_ptr<CarlaLV2ProcessingChain<TimeType, SizeType>>
    create_carla_chain(
        fx_chain_type_t type, size_t sample_rate, std::string title,
        std::shared_ptr<profiling::Profiler> maybe_profiler = nullptr);
};

extern template std::shared_ptr<CarlaLV2ProcessingChain<uint32_t, uint16_t>>
LV2::create_carla_chain(
    fx_chain_type_t type, size_t sample_rate, std::string title,
    std::shared_ptr<profiling::Profiler> maybe_profiler = nullptr);
extern template std::shared_ptr<CarlaLV2ProcessingChain<uint32_t, uint32_t>>
LV2::create_carla_chain(
    fx_chain_type_t type, size_t sample_rate, std::string title,
    std::shared_ptr<profiling::Profiler> maybe_profiler = nullptr);
extern template std::shared_ptr<CarlaLV2ProcessingChain<uint16_t, uint16_t>>
LV2::create_carla_chain(
    fx_chain_type_t type, size_t sample_rate, std::string title,
    std::shared_ptr<profiling::Profiler> maybe_profiler = nullptr);
extern template std::shared_ptr<CarlaLV2ProcessingChain<uint16_t, uint32_t>>
LV2::create_carla_chain(
    fx_chain_type_t type, size_t sample_rate, std::string title,
    std::shared_ptr<profiling::Profiler> maybe_profiler = nullptr);