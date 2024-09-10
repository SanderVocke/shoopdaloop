#pragma once
#include <vector>
#include <cstring>
#include <stdint.h>

template<typename SampleT> class AudioBuffer : public std::vector<SampleT> {
public:
    AudioBuffer(uint32_t size) : std::vector<SampleT>(size) {
        memset((void *)this->data(), 0, sizeof(SampleT) * size);
    }
};