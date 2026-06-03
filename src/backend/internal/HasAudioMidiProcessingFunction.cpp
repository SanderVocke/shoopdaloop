#include "HasAudioProcessingFunction.h"

SHOOP_DEFINE_TYPED_BRIDGE_OBJECT(HasAudioProcessingFunction, ProcessorBridgeStrong, ProcessorBridgeWeak, processor_bridge, processor)

void processor_bridge_proc_process(const ProcessorBridgeWeak &weak, uint32_t nframes) {
    auto processor = processor_bridge_lock(weak);
    if (processor) { processor->PROC_process(nframes); }
}