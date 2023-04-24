#pragma once
#include "LV2SingleProcessingChain.h"

template<typename TimeType, typename SizeType>
class CarlaProcessingChain : public LV2SingleProcessingChain<TimeType, SizeType> {
public:
    enum class CarlaType {
        Rack,
        Patchbay2,
        Patchbay16
    };

};