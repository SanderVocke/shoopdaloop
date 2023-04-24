#pragma once
#include "ProcessingChainInterface.h"
#include <lilv/lilv.h>

template<typename TimeType, typename SizeType>
class LV2SingleProcessingChain : public ProcessingChainInterface<TimeType, SizeType>{

};