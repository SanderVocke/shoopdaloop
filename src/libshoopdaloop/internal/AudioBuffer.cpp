#define IMPLEMENT_AUDIOBUFFER_H
#include "AudioBuffer.h"

template class AudioBuffer<float>;
template class AudioBuffer<int>;

template <typename SampleT>
AudioBuffer<SampleT>::AudioBuffer(size_t size) : std::vector<SampleT>(size) {
  memset((void *)this->data(), 0, sizeof(SampleT) * size);
}