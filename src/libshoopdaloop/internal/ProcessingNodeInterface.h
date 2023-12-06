#pragma once
#include <stdint.h>

class ProcessingNodeInterface {
public:
    virtual void PROC_change_buffer_size(uint32_t buffer_size) {}
    virtual void PROC_change_sample_rate(uint32_t sample_rate) {}
};