#include "MidiMessage.h"

template <typename TimeType, typename SizeType>
MidiMessage<TimeType, SizeType>::MidiMessage(decltype(time) time,
                                             decltype(size) size,
                                             decltype(data) data)
    : time(time), size(size), data(data) {}

template <typename TimeType, typename SizeType>
MidiMessage<TimeType, SizeType>::MidiMessage() {}

template <typename TimeType, typename SizeType>
uint32_t MidiMessage<TimeType, SizeType>::get_time() const {
    return time;
}

template <typename TimeType, typename SizeType>
uint32_t MidiMessage<TimeType, SizeType>::get_size() const {
    return size;
}
template <typename TimeType, typename SizeType>
const uint8_t *MidiMessage<TimeType, SizeType>::get_data() const {
    return data.data();
}
template <typename TimeType, typename SizeType>
void MidiMessage<TimeType, SizeType>::get(uint32_t &size_out,
                                          uint32_t &time_out,
                                          const uint8_t *&data_out) const {
    size_out = size;
    time_out = time;
    data_out = data.data();
}

template class MidiMessage<uint32_t, uint16_t>;
template class MidiMessage<uint32_t, uint32_t>;
template class MidiMessage<uint16_t, uint16_t>;
template class MidiMessage<uint16_t, uint32_t>;