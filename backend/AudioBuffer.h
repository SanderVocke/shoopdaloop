#pragma once
#include <vector>
#include <stdio.h>
#include <cstring>

template<typename SampleT> class AudioBuffer : public std::vector<SampleT> {
public:
    AudioBuffer(size_t size) : std::vector<SampleT>(size) {
        memset((void*)this->data(), 0, sizeof(SampleT)*size);
    }
};