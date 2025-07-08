#include "AudioChannel.h"
#include "AudioPort.h"
#include "types.h"
#include "channel_mode_helpers.h"
#include <boost/lockfree/policies.hpp>
#include <cmath>
#include <cstring>
#include <memory>
#include <stdexcept>
#include <vector>
#include <optional>
#include <boost/lockfree/spsc_queue.hpp>
#include <fmt/format.h>
#include <fmt/ranges.h>

#ifdef _WIN32
#undef min
#undef max
#endif

using namespace std::chrono;
using namespace logging;

template <typename SampleT>
template <size_t N>
inline void AudioChannel<SampleT>::trace_print_data(std::string msg, SampleT *data, size_t len) {
    if (should_log<log_level_debug_trace>()) {
        auto _N = N;
        std::array<SampleT, N> arr;
        std::copy(data, data+std::min(len, N), arr.begin());
        log<log_level_debug_trace>(msg);
        log<log_level_debug_trace>("--> first {} samples: {}", _N, arr);
    }
}

template <typename SampleT>
AudioChannel<SampleT>::Buffers::Buffers(shoop_shared_ptr<BufferPool> pool,
                                        uint32_t initial_max_buffers)
    : pool(pool), buffers_size(pool->object_size()) {
    buffers = shoop_make_shared<std::vector<Buffer>>();
    buffers->reserve(initial_max_buffers);
    reset();
}

template <typename SampleT> void AudioChannel<SampleT>::Buffers::reset() {
    buffers->clear();
    buffers->push_back(get_new_buffer());
}

template <typename SampleT> void AudioChannel<SampleT>::Buffers::set_contents(shoop_shared_ptr<std::vector<Buffer>> contents) {
    buffers = contents;
}

template <typename SampleT> std::vector<SampleT> AudioChannel<SampleT>::Buffers::contiguous_copy(uint32_t max_length) const {
    std::vector<SampleT> rval;
    uint32_t remaining = std::min(n_samples(), max_length);
    rval.reserve(remaining);
    for (auto &buf : *buffers) {
        auto data = buf->data();
        uint32_t step = std::min((size_t) remaining, buf->size());
        rval.insert(rval.end(), data, data + step);
        remaining -= step;
    }
    return rval;
}

template <typename SampleT> AudioChannel<SampleT>::Buffers::Buffers() {
    buffers = shoop_make_shared<std::vector<Buffer>>();
}

template <typename SampleT>
SampleT &AudioChannel<SampleT>::Buffers::at(uint32_t offset) const {
    uint32_t idx = offset / buffers_size;
    if (idx >= buffers->size()) {
        throw_error<std::runtime_error>("OOB buffers access");
    }
    uint32_t head = offset % buffers_size;
    return buffers->at(idx)->at(head);
}

template <typename SampleT>
bool AudioChannel<SampleT>::Buffers::ensure_available(uint32_t offset,
                                                      bool use_pool) {
    uint32_t idx = offset / buffers_size;
    bool changed = false;
    while (buffers->size() <= idx) {
        if (use_pool) {
            buffers->push_back(get_new_buffer());
        } else {
            buffers->push_back(create_new_buffer());
        }
        changed = true;
    }
    return changed;
}

template <typename SampleT>
typename AudioChannel<SampleT>::Buffer
AudioChannel<SampleT>::Buffers::create_new_buffer() const {
    return shoop_make_shared<BufferObj>(buffers_size);
}

template <typename SampleT>
typename AudioChannel<SampleT>::Buffer
AudioChannel<SampleT>::Buffers::get_new_buffer() const {
    if (!pool) {
        throw_error<std::runtime_error>("No pool for buffers allocation");
    }
    auto buf = Buffer(pool->get_object());
    if (buf->size() != buffers_size) {
        throw_error<std::runtime_error>(
            "AudioChannel requires buffers of same length");
    }
    return buf;
}

template <typename SampleT>
uint32_t AudioChannel<SampleT>::Buffers::n_buffers() const {
    return buffers->size();
}

template <typename SampleT>
uint32_t AudioChannel<SampleT>::Buffers::n_samples() const {
    return n_buffers() * buffers_size;
}

template <typename SampleT>
uint32_t
AudioChannel<SampleT>::Buffers::buf_space_for_sample(uint32_t offset) const {
    uint32_t off = offset % buffers_size;
    return buffers_size - off;
}

template <typename SampleT>
typename AudioChannel<SampleT>::Buffers&
AudioChannel<SampleT>::Buffers::operator=(AudioChannel<SampleT>::Buffers const& other) {
    *buffers = *other.buffers;
    pool = other.pool;
    buffers_size = other.buffers_size;
    return *this;
}

template <typename SampleT>
void AudioChannel<SampleT>::throw_if_commands_queued() const {
    if (mp_proc_queue.read_available()) {
        throw_error<std::runtime_error>(
            "Illegal operation while audio channel commands are queued");
    }
}

template <typename SampleT>
AudioChannel<SampleT>::AudioChannel(
    shoop_shared_ptr<BufferPool> buffer_pool, uint32_t initial_max_buffers,
    shoop_channel_mode_t mode)
    : WithCommandQueue(50), ma_buffer_pool(buffer_pool),
      ma_buffers_data_length(0), mp_prerecord_buffers_data_length(0),
      ma_buffer_size(buffer_pool->object_size()),
      mp_recording_source_buffer(nullptr), mp_playback_target_buffer(nullptr),
      mp_playback_target_buffer_size(0), mp_recording_source_buffer_size(0),
      ma_output_peak(0), ma_mode(mode), ma_gain(1.0f), ma_start_offset(0),
      ma_data_seq_nr(0), ma_pre_play_samples(0),
      mp_buffers(buffer_pool, initial_max_buffers),
      mp_prerecord_buffers(buffer_pool, initial_max_buffers),
      mp_prev_process_flags(0), ma_last_played_back_sample(-1) {
}

template <typename SampleT>
void AudioChannel<SampleT>::set_pre_play_samples(uint32_t samples) {
    ma_pre_play_samples = samples;
}

template <typename SampleT>
uint32_t AudioChannel<SampleT>::get_pre_play_samples() const {
    return ma_pre_play_samples;
}

template <typename SampleT>
AudioChannel<SampleT> &
AudioChannel<SampleT>::operator=(AudioChannel<SampleT> const &other) {
    mp_proc_queue.reset();
    if (other.ma_buffer_size != ma_buffer_size) {
        throw_error<std::runtime_error>(
            "Cannot copy audio channels with different buffer sizes.");
    }
    ma_buffer_pool = other.ma_buffer_pool;
    ma_buffers_data_length = other.ma_buffers_data_length.load();
    mp_prerecord_buffers_data_length =
        other.mp_prerecord_buffers_data_length.load();
    ma_start_offset = other.ma_start_offset.load();
    mp_buffers = other.mp_buffers;
    mp_prerecord_buffers = other.mp_prerecord_buffers;
    mp_playback_target_buffer = other.mp_playback_target_buffer;
    mp_playback_target_buffer_size = other.mp_playback_target_buffer_size;
    mp_recording_source_buffer = other.mp_recording_source_buffer;
    mp_recording_source_buffer_size = other.mp_recording_source_buffer_size;
    ma_mode = other.ma_mode.load();
    ma_gain = other.ma_gain.load();
    ma_pre_play_samples = other.ma_pre_play_samples.load();
    mp_prev_process_flags = other.mp_prev_process_flags;
    data_changed();
    return *this;
}

template <typename SampleT>
AudioChannel<SampleT>::AudioChannel() : ma_buffer_size(1) {}

template <typename SampleT> AudioChannel<SampleT>::~AudioChannel() {}

template <typename SampleT> void AudioChannel<SampleT>::data_changed() {
    ma_data_seq_nr++;
}

template <typename SampleT>
void AudioChannel<SampleT>::PROC_process(
    shoop_loop_mode_t mode, std::optional<shoop_loop_mode_t> maybe_next_mode,
    std::optional<uint32_t> maybe_next_mode_delay_cycles,
    std::optional<uint32_t> maybe_next_mode_eta, uint32_t n_samples,
    uint32_t pos_before, uint32_t pos_after, uint32_t length_before,
    uint32_t length_after)
{
    log<log_level_debug_trace>("process");

    // Execute any commands queued from other threads.
    PROC_handle_command_queue();

    auto process_params = get_channel_process_params(
        mode, maybe_next_mode, maybe_next_mode_delay_cycles,
        maybe_next_mode_eta, pos_before, ma_start_offset, ma_mode);
    auto &process_flags = process_params.process_flags;

    // Corner case: a just-created audio channel may immediately
    // go into pre-record mode before being properly added to the
    // processing graph (TODO: better fix).
    // Here, we solve it by only going ahead when buffers have been
    // assigned.
    if (!mp_recording_source_buffer) {
        process_flags &= (~ChannelPreRecord & ~ChannelRecord & ~ChannelReplace);
    }
    if (!mp_playback_target_buffer) {
        process_flags &= (~ChannelPlayback);
    }

    if (!(process_flags & ChannelPreRecord) &&
        (mp_prev_process_flags & ChannelPreRecord)) {
        // Ending pre-record. If transitioning to recording,
        // make our pre-recorded buffers into our main buffers.
        // Otherwise, just discard them.
        if (process_flags & ChannelRecord) {
            log<log_level_debug>(
                "Pre-record end -> carry over to record");
            mp_buffers = mp_prerecord_buffers;
            ma_buffers_data_length = ma_start_offset =
                mp_prerecord_buffers_data_length.load();
            if (mp_buffers.n_buffers() > 0) {
                trace_print_data<16>("Pre-recorded samples carried over:", mp_buffers.buffers->at(0)->data(), mp_buffers.buffers_size);
            }
        } else {
            log<log_level_debug>("Pre-record end -> discard");
        }
        mp_prerecord_buffers.reset();
        mp_prerecord_buffers_data_length = 0;
    }

    if (process_flags & ChannelPlayback) {
        ma_last_played_back_sample = process_params.position;
        PROC_process_playback(
            process_params.position, length_before, n_samples, false,
            mp_playback_target_buffer, mp_playback_target_buffer_size);
    } else {
        ma_last_played_back_sample = -1;
    }
    if (process_flags & ChannelRecord) {
        PROC_process_record(n_samples,
                            ((int)length_before + ma_start_offset),
                            mp_buffers, ma_buffers_data_length,
                            mp_recording_source_buffer,
                            mp_recording_source_buffer_size);
    }
    if (process_flags & ChannelReplace) {
        PROC_process_replace(process_params.position, length_before,
                                n_samples, mp_recording_source_buffer,
                                mp_recording_source_buffer_size);
    }
    if (process_flags & ChannelPreRecord) {
        if (!(mp_prev_process_flags & ChannelPreRecord)) {
            log<log_level_debug>("Pre-record start");
        }
        PROC_process_record(n_samples, mp_prerecord_buffers_data_length,
                            mp_prerecord_buffers,
                            mp_prerecord_buffers_data_length,
                            mp_recording_source_buffer,
                            mp_recording_source_buffer_size);
    }

    mp_prev_process_flags = process_flags;

    // Update recording/playback buffers and ringbuffer.
    if (mp_recording_source_buffer) {
        mp_recording_source_buffer += n_samples;
        mp_recording_source_buffer_size -= n_samples;
    }
    if (mp_playback_target_buffer) {
        mp_playback_target_buffer += n_samples;
        mp_playback_target_buffer_size -= n_samples;
    }
}

template <typename SampleT>
void AudioChannel<SampleT>::PROC_exec_cmd(ProcessingCommand cmd) {
    unsigned ctype = (unsigned)cmd.cmd_type;
    log<log_level_debug_trace>("exec cmd: {}", ctype);
    auto const &rc = cmd.details.raw_copy_details;
    auto const &ac = cmd.details.additive_copy_details;

    if (cmd.cmd_type == ProcessingCommandType::RawCopy) {
        if (!rc.src || !rc.dst) {
            throw_error<std::runtime_error>("Null pointer in raw copy");
        }
        auto n = rc.sz / sizeof(SampleT);
        auto first = (n > 0) ? ((SampleT*)rc.src)[0] : 0.0f;
    }

    float output_peak;

    switch (cmd.cmd_type) {
    case ProcessingCommandType::RawCopy:
        memcpy(rc.dst, rc.src, rc.sz);
        break;
    case ProcessingCommandType::AdditiveCopy:
        output_peak = ma_output_peak.load();
        for (uint32_t i = 0; i < ac.n_elems; i++) {
            auto sample = ac.dst[i] + ac.src[i] * ac.multiplier;
            ac.dst[i] = sample;
            if (ac.update_absmax) {
                output_peak = std::max(output_peak,
                                          (float)std::abs(sample));
            }
        }
        ma_output_peak = output_peak;
        break;
    default:
        throw_error<std::runtime_error>("Unknown processing command");
    };
}

template <typename SampleT>
void AudioChannel<SampleT>::PROC_queue_memcpy(void *dst, void *src, uint32_t sz) {
    log<log_level_debug_trace>("queue memcpy");
    ProcessingCommand cmd;
    cmd.cmd_type = ProcessingCommandType::RawCopy;
    cmd.details.raw_copy_details = {.src = src, .dst = dst, .sz = sz};
    mp_proc_queue.push(cmd);
}

template <typename SampleT>
void AudioChannel<SampleT>::PROC_queue_additivecpy(SampleT *dst, SampleT *src,
                                                   uint32_t n_elems, float mult,
                                                   bool update_absmax) {
    log<log_level_debug_trace>("queue addcpy");
    ProcessingCommand cmd;
    cmd.cmd_type = ProcessingCommandType::AdditiveCopy;
    cmd.details.additive_copy_details = {.src = src,
                                         .dst = dst,
                                         .multiplier = mult,
                                         .n_elems = n_elems,
                                         .update_absmax = update_absmax};
    mp_proc_queue.push(cmd);
}

template <typename SampleT>
void AudioChannel<SampleT>::PROC_finalize_process() {
    log<log_level_debug_trace>("finalize");

    ProcessingCommand cmd;
    while (mp_proc_queue.pop(cmd)) {
        PROC_exec_cmd(cmd);
    }
}


template <typename SampleT>
void AudioChannel<SampleT>::adopt_ringbuffer_contents(
    shoop_shared_ptr<PortInterface> from_port,
    std::optional<unsigned> reverse_start_offset,
    std::optional<unsigned> keep_samples_before_start_offset,
    bool thread_safe)
{
    if (reverse_start_offset.has_value()) {
        log<log_level_debug>("queue adopt ringbuffer @ reverse offset {}", reverse_start_offset.value());
    } else {
        log<log_level_debug>("queue adopt ringbuffer @ begin");
    }
    auto audioport = shoop_dynamic_pointer_cast<AudioPort<SampleT>>(from_port);
    if (!audioport) {
        log<log_level_error>("Cannot adopt audio ringbuffer from non-audio port");
        return;
    }

    auto fn = [audioport, reverse_start_offset, keep_samples_before_start_offset, this]() {
        auto data = audioport->PROC_get_ringbuffer_contents();
        auto ori_len = data.n_samples;
        int so = reverse_start_offset.has_value() ? (int)data.n_samples - (int)reverse_start_offset.value() : 0;
        // Remove data as allowed
        if (keep_samples_before_start_offset.has_value()) {
            unsigned keep = keep_samples_before_start_offset.value();
            while(so > (int)(keep + data.buffer_size)) {
                so -= data.buffer_size;
                data.n_samples -= data.buffer_size;
                data.data->erase(data.data->begin());
            }
        }

        mp_buffers.set_contents(data.data);
        set_start_offset(so);
        PROC_set_length(data.n_samples);
        log<log_level_debug_trace>("adopted ringbuffer: {} of {} samples stored, start offset {}", data.n_samples, ori_len, so);
        data_changed();
    };

    if (thread_safe) {
        queue_process_thread_command(fn);
    } else {
        fn();
    }
}

template <typename SampleT>
void AudioChannel<SampleT>::load_data(SampleT *samples, uint32_t len,
                                      bool thread_safe) {
    log<log_level_debug_trace>("load data ({} samples)", len);

    // Convert to internal storage layout
    auto buffers =
        Buffers(ma_buffer_pool, std::ceil((float)len / (float)ma_buffer_size));
    buffers.ensure_available(len, false);

    for (uint32_t idx = 0; idx < buffers.n_buffers(); idx++) {
        auto &buf = buffers.buffers->at(idx);
        buf = shoop_make_shared<AudioBuffer<SampleT>>(ma_buffer_size);
        uint32_t already_copied = idx * ma_buffer_size;
        uint32_t n_elems = std::min(ma_buffer_size, len - already_copied);
        memcpy(buf->data(), samples + (idx * ma_buffer_size),
               sizeof(SampleT) * n_elems);
    }

    auto cmd = [this, buffers, len]() {
        mp_buffers = buffers;
        ma_buffers_data_length = len;
        mp_prerecord_buffers_data_length = 0;
        ma_start_offset = 0;
        data_changed();
    };

    if (thread_safe) {
        exec_process_thread_command(cmd);
    } else {
        cmd();
    }
}

template <typename SampleT>
std::vector<SampleT> AudioChannel<SampleT>::get_data(bool thread_safe) {
    Buffers buffers;
    uint32_t length;
    auto cmd = [this, &buffers, &length]() {
        buffers = mp_buffers;
        length = ma_buffers_data_length;
    };

    if (thread_safe) {
        exec_process_thread_command(cmd);
    } else {
        cmd();
    }

    std::vector<SampleT> rval = buffers.contiguous_copy(length);
    return rval;
}

template <typename SampleT>
void AudioChannel<SampleT>::PROC_process_record(
    uint32_t n_samples,
    uint32_t record_from,
    Buffers &buffers,
    std::atomic<uint32_t>
    &buffers_data_length,
    SampleT *record_buffer,
    uint32_t record_buffer_size) {
    log<log_level_debug_trace>("process record: {} samples @ {}", n_samples, record_from);

    if (record_buffer_size < n_samples) {
        throw_error<std::runtime_error>(
            "Attempting to record out of bounds of input buffer");
    }
    bool changed = false;
    auto data_length = buffers_data_length.load();

    auto &from = record_buffer;

    // Find the position in our sequence of buffers (buffer index and index in
    // buffer)
    buffers.ensure_available(record_from + n_samples);
    SampleT *ptr = &buffers.at(record_from);
    uint32_t buf_space = buffers.buf_space_for_sample(record_from);

    // Record all, or to the end of the current buffer, whichever
    // comes first
    auto n = std::min(n_samples, buf_space);
    auto rest = n_samples - n;

    // Queue the actual copy for later.
    PROC_queue_memcpy((void *)ptr, (void *)from, sizeof(SampleT) * n);
    changed = changed || (n > 0);

    buffers_data_length = record_from + n;

    // If we reached the end, add another buffer
    // and record the rest.
    if (changed) {
        data_changed();
    }
    if (rest > 0) {
        PROC_process_record(rest, record_from + n, buffers, buffers_data_length,
                            record_buffer + n, record_buffer_size - n);
    }
}

template <typename SampleT>
void AudioChannel<SampleT>::PROC_process_replace(uint32_t data_position, uint32_t length,
                                                 uint32_t n_samples,
                                                 SampleT *record_buffer,
                                                 uint32_t record_buffer_size) {
    log<log_level_debug_trace>("process replace");

    if (record_buffer_size < n_samples) {
        throw_error<std::runtime_error>(
            "Attempting to replace out of bounds of recording buffer");
    }
    auto data_length = ma_buffers_data_length.load();
    auto start_offset = ma_start_offset.load();
    bool changed = false;

    if (data_position < 0) {
        // skip ahead to the part that is in range
        const int skip = -data_position;
        const int n = (int)n_samples - skip;
        record_buffer += std::min(skip, (int)record_buffer_size);
        record_buffer_size = std::max((int)record_buffer_size - skip, 0);
        return PROC_process_replace(data_position + skip, length,
                                    std::max((int)n_samples - skip, 0),
                                    record_buffer, record_buffer_size);
    }

    if (mp_buffers.ensure_available(data_position + n_samples)) {
        data_length = ma_buffers_data_length;
        changed = true;
    }

    uint32_t samples_left = data_length - data_position;
    uint32_t buf_space = mp_buffers.buf_space_for_sample(data_position);
    SampleT *to = &mp_buffers.at(data_position);
    SampleT *&from = record_buffer;
    auto n = std::min({buf_space, samples_left, n_samples});
    auto rest = n_samples - n;

    // Queue the actual copy for later
    PROC_queue_memcpy((void *)to, (void *)from, sizeof(SampleT) * n);
    changed = changed || (n > 0);

    if (changed) {
        data_changed();
    }

    // If we didn't replace all yet, go to next buffer and continue
    if (rest > 0) {
        if (n == 0 && data_position >= data_length) {
            throw_error<std::runtime_error>("Attempt to replace out of bounds");
        }
        PROC_process_replace(data_position + n, length, rest, record_buffer + n,
                             record_buffer_size - n);
    }
}

template <typename SampleT> uint32_t AudioChannel<SampleT>::get_length() const {
    return ma_buffers_data_length;
}

template <typename SampleT>
void AudioChannel<SampleT>::PROC_set_length(uint32_t length) {
    log<log_level_debug_trace>("set length -> {}", length);
    ma_buffers_data_length = length;
    data_changed();
}

template <typename SampleT>
SampleT const &AudioChannel<SampleT>::PROC_at(uint32_t position) const {
    return mp_buffers.at(position);
}

template <typename SampleT>
void AudioChannel<SampleT>::PROC_process_playback(int data_position,
                                                  uint32_t length,
                                                  uint32_t n_samples, bool muted,
                                                  SampleT *playback_buffer,
                                                  uint32_t playback_buffer_size) {
    log<log_level_debug_trace>("process playback");

    if (playback_buffer_size < n_samples) {
        throw_error<std::runtime_error>(
            "Attempting to play out of bounds of target buffer");
    }

    auto data_length = ma_buffers_data_length.load();
    auto start_offset = ma_start_offset.load();

    const int starting_data_position =
        std::max(0, (int)ma_start_offset - (int)ma_pre_play_samples);
    const int skip = std::max(0, starting_data_position - data_position);
    if (skip > 0) {
        data_position += skip;
        n_samples = std::max((int)n_samples - skip, 0);
        playback_buffer += std::min(skip, (int)playback_buffer_size);
        playback_buffer_size = std::max((int)playback_buffer_size - skip, 0);
    }

    if (data_position < data_length) {
        // We have something to play.
        uint32_t buf_space = mp_buffers.buf_space_for_sample(data_position);
        SampleT *from = &mp_buffers.at(data_position);
        SampleT *&to = playback_buffer;
        auto n = std::min({buf_space, n_samples});
        auto rest = n_samples - n;

        if (!muted) {
            PROC_queue_additivecpy(to, from, n, ma_gain, true);
        }

        // If we didn't play back all yet, go to next buffer and continue
        if (rest > 0) {
            PROC_process_playback(data_position + n, length, rest, muted,
                                  playback_buffer + n,
                                  playback_buffer_size - n);
        }
    }
}

template <typename SampleT>
std::optional<uint32_t> AudioChannel<SampleT>::PROC_get_next_poi(
    shoop_loop_mode_t mode, std::optional<shoop_loop_mode_t> maybe_next_mode,
    std::optional<uint32_t> maybe_next_mode_delay_cycles,
    std::optional<uint32_t> maybe_next_mode_eta, uint32_t length,
    uint32_t position) const {
    std::optional<uint32_t> rval = std::nullopt;
    auto merge_poi = [&rval](uint32_t poi) {
        rval = rval.has_value() ? std::min(rval.value(), poi) : poi;
    };

    auto process_params = get_channel_process_params(
        mode, maybe_next_mode, maybe_next_mode_delay_cycles,
        maybe_next_mode_eta, position, ma_start_offset, ma_mode);

    if (ma_mode) {
        if (process_params.process_flags & ChannelPlayback) {
            merge_poi(mp_playback_target_buffer_size);
        }
        if (process_params.process_flags & (ChannelRecord | ChannelPreRecord)) {
            merge_poi(mp_recording_source_buffer_size);
        }
    }
    return rval;
}

template <typename SampleT>
void AudioChannel<SampleT>::PROC_handle_poi(shoop_loop_mode_t mode, uint32_t length,
                                            uint32_t position){};

template <typename SampleT>
void AudioChannel<SampleT>::PROC_set_playback_buffer(SampleT *buffer,
                                                     uint32_t size) {
    log<log_level_debug_trace>("set playback buf ({} samples)", size);

    throw_if_commands_queued();
    mp_playback_target_buffer = buffer;
    mp_playback_target_buffer_size = size;
}

template <typename SampleT>
void AudioChannel<SampleT>::PROC_set_recording_buffer(SampleT *buffer,
                                                      uint32_t size) {
    log<log_level_debug_trace>("set rec buf ({} samples)", size);

    throw_if_commands_queued();
    mp_recording_source_buffer = buffer;
    mp_recording_source_buffer_size = size;
}

template <typename SampleT>
void AudioChannel<SampleT>::PROC_clear(uint32_t length) {
    log<log_level_debug_trace>("clear");

    throw_if_commands_queued();
    mp_buffers.ensure_available(length);
    ma_buffers_data_length = length;
    ma_start_offset = 0;
    data_changed();
}

template <typename SampleT>
float AudioChannel<SampleT>::get_output_peak() const {
    return ma_output_peak;
}

template <typename SampleT> void AudioChannel<SampleT>::reset_output_peak() {
    ma_output_peak = 0.0f;
}

template <typename SampleT>
void AudioChannel<SampleT>::set_mode(shoop_channel_mode_t mode) {
    ma_mode = mode;
}

template <typename SampleT>
shoop_channel_mode_t AudioChannel<SampleT>::get_mode() const {
    return ma_mode;
}

template <typename SampleT>
void AudioChannel<SampleT>::set_gain(float gain) {
    ma_gain = gain;
}

template <typename SampleT> float AudioChannel<SampleT>::get_gain() {
    return ma_gain;
}

template <typename SampleT>
void AudioChannel<SampleT>::set_start_offset(int offset) {
    ma_start_offset = offset;
}

template <typename SampleT>
int AudioChannel<SampleT>::get_start_offset() const {
    return ma_start_offset;
}

template <typename SampleT>
unsigned AudioChannel<SampleT>::get_data_seq_nr() const {
    return ma_data_seq_nr;
}

template <typename SampleT>
std::optional<uint32_t> AudioChannel<SampleT>::get_played_back_sample() const {
    auto v = ma_last_played_back_sample.load();
    if (v >= 0) {
        return v;
    }
    return std::nullopt;
}

template <typename SampleT>
typename AudioChannel<SampleT>::Buffer
AudioChannel<SampleT>::get_new_buffer() const {
    auto buf = Buffer(ma_buffer_pool->get_object());
    if (buf->size() != ma_buffer_size) {
        throw_error<std::runtime_error>(
            "AudioChannel requires buffers of same length");
    }
    return buf;
}

template class AudioChannel<float>;
template class AudioChannel<int>;