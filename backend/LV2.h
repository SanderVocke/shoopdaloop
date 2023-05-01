#pragma once
#include "CarlaLV2ProcessingChain.h"
#include "types.h"
#include <lilv/lilv.h>
#include <memory>

class LV2 {
    LilvWorld *m_world;

public:
    LV2() {
        m_world = lilv_world_new();
        lilv_world_load_all(m_world);
    }

    ~LV2() {
        lilv_world_free(m_world);
    }

    template<typename TimeType, typename SizeType>
    std::shared_ptr<CarlaLV2ProcessingChain<TimeType, SizeType>> create_carla_chain(
        fx_chain_type_t type,
        size_t sample_rate,
        std::string title
    ) {
        return std::make_shared<CarlaLV2ProcessingChain<TimeType, SizeType>>
            (m_world, type, sample_rate, title);
    }
};