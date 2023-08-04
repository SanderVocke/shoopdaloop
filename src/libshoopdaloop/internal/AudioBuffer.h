#pragma once
#include <vector>

template<typename SampleT> class AudioBuffer : public std::vector<SampleT> {
public:
    AudioBuffer(size_t size);
};

#ifndef IMPLEMENT_AUDIOBUFFER_H
extern template class AudioBuffer<float>;
extern template class AudioBuffer<int>;
#endif