#pragma once
#include <vector>
#include <stdio.h>

template<typename SampleT> class AudioBuffer : public std::vector<SampleT> {
public:
    AudioBuffer(size_t size) : std::vector<SampleT>(size) {}
};