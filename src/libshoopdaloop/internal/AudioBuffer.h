#pragma once
#include <vector>
#include <stdio.h>
#include <cstring>

template<typename SampleT> class AudioBuffer : public std::vector<SampleT> {
public:
    AudioBuffer(size_t size);
};

#ifndef IMPLEMENT_AUDIOBUFFER_H
extern template class AudioBuffer<float>;
extern template class AudioBuffer<int>;
#endif