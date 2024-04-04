#include "MidiRingbuffer.h"
#include "types.h"
#include <algorithm>

template<typename TimeType, typename SizeType>
MidiRingbuffer<TimeType, SizeType>::MidiRingbuffer(uint32_t data_size)
    : Storage(data_size)
    , n_samples(0)
    , current_buffer_start_time(0)
    , current_buffer_end_time(0)
{}

template<typename TimeType, typename SizeType>
void MidiRingbuffer<TimeType, SizeType>::set_n_samples(uint32_t n) {
    n_samples = n;
    auto end = current_buffer_end_time.load();
    Storage::truncate(end - std::min((uint32_t) end, n), Storage::TruncateType::TruncateTail);
}

template<typename TimeType, typename SizeType>
void MidiRingbuffer<TimeType, SizeType>::next_buffer(TimeType n_frames) {
    TimeType old_end = current_buffer_end_time.load();
    TimeType new_end = old_end + n_frames;
    current_buffer_start_time = old_end;
    current_buffer_end_time = new_end;
    Storage::truncate(new_end - std::min((TimeType)n_samples.load(), new_end), Storage::TruncateType::TruncateTail);
}

template<typename TimeType, typename SizeType>
bool MidiRingbuffer<TimeType, SizeType>::put(TimeType frame_in_current_buffer, SizeType size, const uint8_t* data) {
    TimeType time = current_buffer_start_time + frame_in_current_buffer;
    if (time > current_buffer_end_time) {
        Storage::template log<log_level_error>("MidiRingbuffer::put: time is out of range");
        return false;
    }
    return Storage::append(time, size, data, true);
}

template<typename TimeType, typename SizeType>
TimeType MidiRingbuffer<TimeType, SizeType>::snapshot(Storage &target) const {
    Storage::copy(target);
    return current_buffer_end_time.load();
}

template class MidiRingbuffer<uint32_t, uint16_t>;
template class MidiRingbuffer<uint32_t, uint32_t>;
template class MidiRingbuffer<uint16_t, uint16_t>;
template class MidiRingbuffer<uint16_t, uint32_t>;