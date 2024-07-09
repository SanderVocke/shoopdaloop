#include "MidiRingbuffer.h"
#include "types.h"
#include <algorithm>
#include <cstdint>

MidiRingbuffer::MidiRingbuffer(uint32_t data_size)
    : Storage(data_size)
    , n_samples(0)
    , current_buffer_start_time(0)
    , current_buffer_end_time(0)
{}

void MidiRingbuffer::set_n_samples(uint32_t n) {
    n_samples = n;
    auto end = current_buffer_end_time.load();
    Storage::truncate(end - std::min((uint32_t) end, n), Storage::TruncateType::TruncateTail);
}

uint32_t MidiRingbuffer::get_n_samples() const {
    return n_samples.load();
}

void MidiRingbuffer::next_buffer(uint32_t n_frames) {
    uint32_t old_end = current_buffer_end_time.load();
    uint32_t new_end = old_end + n_frames;
    Storage::template log<log_level_debug_trace>("MidiRingbuffer - next buffer: {} -> {}", old_end, new_end);
    current_buffer_start_time = old_end;
    current_buffer_end_time = new_end;
    Storage::truncate(new_end - std::min((uint32_t)n_samples.load(), new_end), Storage::TruncateType::TruncateTail);
}

bool MidiRingbuffer::put(uint32_t frame_in_current_buffer, uint16_t size, const uint8_t* data) {
    uint32_t time = current_buffer_start_time + frame_in_current_buffer;
    if (time > current_buffer_end_time) {
        Storage::template log<log_level_error>("MidiRingbuffer::put: time is out of range");
        return false;
    }
    auto rval = Storage::append(time, size, data, true);
    auto n = Storage::n_events();
    Storage::template log<log_level_debug_trace>("MidiRingbuffer - put at time: {}, # msgs is {}", time, n);
    return rval;
}

uint32_t MidiRingbuffer::snapshot(MidiStorage &target) const {
    Storage::copy(target);
    return current_buffer_end_time.load();
}