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
    Storage::truncate(end - std::min((uint32_t) end, n), Storage::TruncateSide::TruncateTail);
}

uint32_t MidiRingbuffer::get_n_samples() const {
    return n_samples.load();
}

void MidiRingbuffer::next_buffer(uint32_t n_frames) {
    uint32_t old_end = current_buffer_end_time.load();
    uint32_t new_end = old_end + n_frames;  // Note: may overflow

    if (new_end < old_end) {
        // Overflow occurring. We will shift the timestamps of the whole buffer such that all messages
        // again have a low time value.
        Storage::template log<log_level_debug_trace>("MidiRingbuffer - time overflow, resetting");
        const uint32_t moved_new_end = n_samples.load();
        const int shift = (int)moved_new_end - (int)new_end;
        Storage::for_each_msg_modify([shift](uint32_t &t, uint16_t &s, uint8_t *d) {
            t += shift;
        });
        new_end += shift;
        old_end += shift;
    }

    Storage::truncate(new_end - std::min(n_samples.load(), new_end), Storage::TruncateSide::TruncateTail);
    Storage::template log<log_level_debug_trace>("MidiRingbuffer - next buffer: {} -> {}", old_end, new_end);
    current_buffer_start_time = old_end;
    current_buffer_end_time = new_end;
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

void MidiRingbuffer::snapshot(MidiStorage &target, std::optional<uint32_t> start_offset_from_end) const {
    Storage::copy(target);
    auto const end = current_buffer_end_time.load();
    auto const start_from_end = start_offset_from_end.value_or(n_samples.load());
    const uint32_t min_message_time = end - std::min(end, start_from_end);
    target.truncate(min_message_time, Storage::TruncateSide::TruncateTail);
    target.for_each_msg_modify([min_message_time](uint32_t &t, uint16_t &s, uint8_t *d) {
        t -= min_message_time;
    });
}

uint32_t MidiRingbuffer::get_current_start_time() const {
    return current_buffer_end_time.load() - n_samples.load();
}

uint32_t MidiRingbuffer::get_current_end_time() const {
    return current_buffer_end_time.load();
}