#pragma once
#include "CarlaLV2ProcessingChain.h"
#include <memory>
#include "LoggingEnabled.h"

class LV2 : private ModuleLoggingEnabled<"Backend.LV2"> {
    LilvWorld *m_world = nullptr;

  public:
    LV2();
    ~LV2();

    template <typename TimeType, typename SizeType>
    std::shared_ptr<CarlaLV2ProcessingChain<TimeType, SizeType>>
    create_carla_chain(
        shoop_fx_chain_type_t type, uint32_t sample_rate, uint32_t buffer_size, std::string title);
};

extern template std::shared_ptr<CarlaLV2ProcessingChain<uint32_t, uint16_t>>
LV2::create_carla_chain(
    shoop_fx_chain_type_t type, uint32_t sample_rate, uint32_t buffer_size, std::string title);
extern template std::shared_ptr<CarlaLV2ProcessingChain<uint32_t, uint32_t>>
LV2::create_carla_chain(
    shoop_fx_chain_type_t type, uint32_t sample_rate, uint32_t buffer_size, std::string title);
extern template std::shared_ptr<CarlaLV2ProcessingChain<uint16_t, uint16_t>>
LV2::create_carla_chain(
    shoop_fx_chain_type_t type, uint32_t sample_rate, uint32_t buffer_size, std::string title);
extern template std::shared_ptr<CarlaLV2ProcessingChain<uint16_t, uint32_t>>
LV2::create_carla_chain(
    shoop_fx_chain_type_t type, uint32_t sample_rate, uint32_t buffer_size, std::string title);