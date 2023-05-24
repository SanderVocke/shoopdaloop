#pragma once
#include "ChannelInterface.h"
#include "ObjectPool.h"
#include "AudioBuffer.h"
#include "WithCommandQueue.h"
#include "types.h"
#include "channel_mode_helpers.h"
#include <boost/lockfree/policies.hpp>
#include <cmath>
#include <cstring>
#include <memory>
#include <stdexcept>
#include <thread>
#include <vector>
#include <optional>
#include <iostream>
#include <chrono>
#include <boost/lockfree/spsc_queue.hpp>

using namespace std::chrono_literals;

template<typename SampleT>
class AudioChannel : public ChannelInterface,
                            private WithCommandQueue<10, 1000, 1000> {
public:
    typedef AudioBuffer<SampleT> BufferObj;
    typedef ObjectPool<BufferObj> BufferPool;
    typedef std::shared_ptr<BufferObj> Buffer;
    
private:
    struct Buffers;

    // Members which may be accessed from any thread (ma prefix)
    std::shared_ptr<BufferPool> ma_buffer_pool;
    const size_t ma_buffer_size;
    std::atomic<int> ma_start_offset;
    std::atomic<size_t> ma_pre_play_samples;
    std::atomic<float> ma_output_peak;
    std::atomic<float> ma_volume;
    std::atomic<channel_mode_t> ma_mode;
    std::atomic<unsigned> ma_data_seq_nr;

    // Members which may be accessed from the process thread only (mp prefix)
    Buffers mp_buffers; // Buffers holding main audio data
    std::atomic<size_t> ma_buffers_data_length;
    Buffers mp_secondary_buffers; // For temporarily holding pre-recorded data before fully entering record mode
    std::atomic<size_t> ma_secondary_buffers_data_length;

    SampleT *mp_playback_target_buffer;
    size_t   mp_playback_target_buffer_size;
    SampleT *mp_recording_source_buffer;
    size_t   mp_recording_source_buffer_size;

    unsigned mp_prev_process_flags;

    enum class ProcessingCommandType {
        RawCopy,
        AdditiveCopy
    };

    struct RawCopyDetails {
        void* src;
        void* dst;
        size_t sz;
    };

    struct AdditiveCopyDetails {
        SampleT* src;
        SampleT* dst;
        float multiplier;
        size_t n_elems;
        bool update_absmax;
    };

    struct Buffers {
        size_t buffers_size;
        std::vector<Buffer> buffers;
        std::shared_ptr<BufferPool> pool;

        Buffers(std::shared_ptr<BufferPool> pool, size_t initial_max_buffers)
            : pool(pool),
              buffers_size(pool->object_size())
        {
            buffers.reserve(initial_max_buffers);
            reset();
        }

        void reset() {
            buffers.clear();
            buffers.push_back(get_new_buffer());
        }

        Buffers() {}

        SampleT &at(size_t offset) {
            size_t idx = offset / buffers_size;
            if (idx >= buffers.size()) { throw std::runtime_error("OOB buffers access"); }
            size_t head = offset % buffers_size;
            return buffers.at(idx)->at(head);
        }

        bool ensure_available(size_t offset, bool use_pool=true) {
            size_t idx = offset / buffers_size;
            bool changed = false;
            while(buffers.size() <= idx) {
                if (use_pool) {
                    buffers.push_back(get_new_buffer());
                } else {
                    buffers.push_back(create_new_buffer());
                }
                changed = true;
            }
            return changed;
        }

        Buffer create_new_buffer() const {
            return std::make_shared<BufferObj>(buffers_size);
        }

        Buffer get_new_buffer() const {
            if (!pool) {
                throw std::runtime_error("No pool for buffers allocation");
            }
            auto buf = Buffer(pool->get_object());
            if (buf->size() != buffers_size) {
                throw std::runtime_error("AudioChannel requires buffers of same length");
            }
            return buf;
        }

        size_t n_buffers() const { return buffers.size(); }
        size_t n_samples() const { return n_buffers() * buffers_size; }
        size_t buf_space_for_sample(size_t offset) const {
            size_t off = offset % buffers_size;
            return buffers_size - off;
        }
    };

    union ProcessingCommandDetails {
        RawCopyDetails raw_copy_details;
        AdditiveCopyDetails additive_copy_details;
    };

    struct ProcessingCommand {
        ProcessingCommandType cmd_type;
        ProcessingCommandDetails details;
    };

    using ProcessingQueue = boost::lockfree::spsc_queue<ProcessingCommand, boost::lockfree::capacity<16>>;
    ProcessingQueue mp_proc_queue;

    void throw_if_commands_queued() const {
        if(mp_proc_queue.read_available()) { throw std::runtime_error("Illegal operation while audio channel commands are queued"); }
    }

public:

    AudioChannel(
            std::shared_ptr<BufferPool> buffer_pool,
            size_t initial_max_buffers,
            channel_mode_t mode) :
        WithCommandQueue<10, 1000, 1000>(),
        ma_buffer_pool(buffer_pool),
        ma_buffers_data_length(0),
        ma_secondary_buffers_data_length(0),
        ma_buffer_size(buffer_pool->object_size()),
        mp_recording_source_buffer(nullptr),
        mp_playback_target_buffer(nullptr),
        mp_playback_target_buffer_size(0),
        mp_recording_source_buffer_size(0),
        ma_output_peak(0),
        ma_mode(mode),
        ma_volume(1.0f),
        ma_start_offset(0),
        ma_data_seq_nr(0),
        ma_pre_play_samples(0),
        mp_buffers(buffer_pool, initial_max_buffers),
        mp_secondary_buffers(buffer_pool, initial_max_buffers),
        mp_prev_process_flags(0)
    {}

    virtual void set_pre_play_samples(size_t samples) override {
        ma_pre_play_samples = samples;
    }

    virtual size_t get_pre_play_samples() const override {
        return ma_pre_play_samples;
    }

    // NOTE: only use on process thread!
    AudioChannel<SampleT>& operator= (AudioChannel<SampleT> const& other) {
        mp_proc_queue.reset();
        if (other.ma_buffer_size != ma_buffer_size) {
            throw std::runtime_error("Cannot copy audio channels with different buffer sizes.");
        }
        ma_buffer_pool = other.ma_buffer_pool;
        ma_buffers_data_length = other.ma_buffers_data_length.load();
        ma_secondary_buffers_data_length = other.ma_secondary_buffers_data_length.load();
        ma_start_offset = other.ma_start_offset.load();
        mp_buffers = other.mp_buffers;
        mp_secondary_buffers = other.mp_secondary_buffers;
        mp_playback_target_buffer = other.mp_playback_target_buffer;
        mp_playback_target_buffer_size = other.mp_playback_target_buffer_size;
        mp_recording_source_buffer = other.mp_recording_source_buffer;
        mp_recording_source_buffer_size = other.mp_recording_source_buffer_size;
        ma_mode = other.ma_mode;
        ma_volume = other.ma_volume;
        ma_pre_play_samples = other.ma_pre_play_samples;
        mp_prev_process_flags = other.mp_prev_process_flags;
        data_changed();
        return *this;
    }

    AudioChannel() = default;
    ~AudioChannel() override {}

    void data_changed() {
        ma_data_seq_nr++;
    }

    void PROC_process(
        loop_mode_t mode,
        std::optional<loop_mode_t> maybe_next_mode,
        std::optional<size_t> maybe_next_mode_delay_cycles,
        std::optional<size_t> maybe_next_mode_eta,
        size_t n_samples,
        size_t pos_before,
        size_t pos_after,
        size_t length_before,
        size_t length_after
    ) override {
        // Execute any commands queued from other threads.
        PROC_handle_command_queue();

        auto process_params = get_channel_process_params(
            mode,
            maybe_next_mode,
            maybe_next_mode_delay_cycles,
            maybe_next_mode_eta,
            pos_before,
            ma_start_offset,
            ma_mode
        );
        auto const& process_flags = process_params.process_flags;

        if (!(process_flags & ChannelPreRecord) &&
             (mp_prev_process_flags & ChannelPreRecord))
        {
            // Ending pre-record. If transitioning to recording,
            // make our pre-recorded buffers into our main buffers.
            // Otherwise, just discard them.
            if (process_flags & ChannelRecord) {
                mp_buffers = mp_secondary_buffers;
                ma_buffers_data_length = ma_start_offset = ma_secondary_buffers_data_length.load();
            }
            mp_secondary_buffers.reset();
            ma_secondary_buffers_data_length = 0;
        }

        if (process_flags & ChannelPlayback) {
            PROC_process_playback(process_params.position, length_before, n_samples, false);
        }
        if (process_flags & ChannelRecord) {
            PROC_process_record(n_samples, ((int)length_before + ma_start_offset), mp_buffers, ma_buffers_data_length);
        }
        if (process_flags & ChannelReplace) {
            PROC_process_replace(process_params.position, length_before, n_samples);
        }
        if (process_flags & ChannelPreRecord) {
            PROC_process_record(n_samples, ma_secondary_buffers_data_length, mp_secondary_buffers, ma_secondary_buffers_data_length);
        }

        mp_prev_process_flags = process_flags;
    }

    void PROC_exec_cmd(ProcessingCommand cmd) {
        auto const& rc = cmd.details.raw_copy_details;
        auto const& ac = cmd.details.additive_copy_details;
        switch (cmd.cmd_type) {
        case ProcessingCommandType::RawCopy:
            memcpy(rc.dst, rc.src, rc.sz);
            break;
        case ProcessingCommandType::AdditiveCopy:
            for(size_t i=0; i<ac.n_elems; i++) {
                auto sample = ac.dst[i] + ac.src[i] * ac.multiplier;
                ac.dst[i] = sample;
                if (ac.update_absmax) { ma_output_peak = std::max((float)ma_output_peak.load(), (float)std::abs(sample));}
            }
            break;
        default:
            throw std::runtime_error("Unknown processing command");
        };
    }

    void PROC_queue_memcpy(void* dst, void* src, size_t sz) {
        ProcessingCommand cmd;
        cmd.cmd_type = ProcessingCommandType::RawCopy;
        cmd.details.raw_copy_details = {
            .src = src,
            .dst = dst,
            .sz = sz
        };
        mp_proc_queue.push(cmd);
    }

    void PROC_queue_additivecpy(SampleT* dst, SampleT* src, size_t n_elems, float mult, bool update_absmax) {
        ProcessingCommand cmd;
        cmd.cmd_type = ProcessingCommandType::AdditiveCopy;
        cmd.details.additive_copy_details = {
            .src = src,
            .dst = dst,
            .n_elems = n_elems,
            .multiplier = mult,
            .update_absmax = update_absmax
        };
        mp_proc_queue.push(cmd);
    }

    void PROC_finalize_process() override {
        ProcessingCommand cmd;
        while(mp_proc_queue.pop(cmd)) {
            PROC_exec_cmd(cmd);
        }
    }

    // Load data into the loop. Should always be called outside
    // the processing thread.
    void load_data(SampleT* samples, size_t len, bool thread_safe = true) {
        // Convert to internal storage layout
        auto buffers = Buffers(ma_buffer_pool, std::ceil((float)len / (float)ma_buffer_size));
        buffers.ensure_available(len, false);

        for (size_t idx=0; idx < buffers.n_buffers(); idx++) {
            auto &buf = buffers.buffers.at(idx);
            buf = std::make_shared<AudioBuffer<SampleT>>(ma_buffer_size);
            size_t already_copied = idx*ma_buffer_size;
            size_t n_elems = std::min(ma_buffer_size, len - already_copied);
            memcpy(buf->data(), samples + (idx * ma_buffer_size), sizeof(SampleT)*n_elems);
        }

        auto cmd = [=]() {
            mp_buffers = buffers;
            ma_buffers_data_length = len;
            ma_secondary_buffers_data_length = 0;
            ma_start_offset = 0;
            data_changed();
        };

        if (thread_safe) {
            exec_process_thread_command(cmd);
        } else {
            cmd();
        }
    }

    // Get loop data. Should always be called outside
    // the processing thread.
    std::vector<SampleT> get_data(bool thread_safe = true) {
        Buffers buffers;
        size_t length;
        auto cmd = [this, &buffers, &length]() {
            buffers = mp_buffers;
            length = ma_buffers_data_length;
        };

        if (thread_safe) {
            exec_process_thread_command(cmd);
        } else {
            cmd();
        }

        std::vector<SampleT> rval(length);
        for(size_t idx=0; idx<length; idx++) {
            rval[idx] = buffers.at(idx);
        }
        return rval;
    }

    void PROC_process_record(
        size_t n_samples,
        size_t record_from,
        Buffers &buffers,
        std::atomic<size_t> &buffers_data_length
    ) {
        if (mp_recording_source_buffer_size < n_samples) {
            throw std::runtime_error("Attempting to record out of bounds of input buffer");
        }
        bool changed = false;
        auto data_length = buffers_data_length.load();

        auto &from = mp_recording_source_buffer;

        // Find the position in our sequence of buffers (buffer index and index in buffer)
        buffers.ensure_available(record_from + n_samples);
        SampleT *ptr = &buffers.at(record_from);
        size_t buf_space = buffers.buf_space_for_sample(record_from);
        
        // Record all, or to the end of the current buffer, whichever
        // comes first
        auto n = std::min(n_samples, buf_space);
        auto rest = n_samples - n;

        // Queue the actual copy for later.
        PROC_queue_memcpy((void*) ptr, (void*)from, sizeof(SampleT)*n);
        changed = changed || (n > 0);

        mp_recording_source_buffer += n;
        mp_recording_source_buffer_size -= n;
        buffers_data_length = record_from + n;

        // If we reached the end, add another buffer
        // and record the rest.
        size_t record_from_next = record_from + n;
        if (changed) { data_changed(); }
        if(rest > 0) {
            PROC_process_record(rest, record_from_next, buffers, buffers_data_length);
        }
    }

    void PROC_process_replace(size_t position, size_t length, size_t n_samples) {
        if (mp_recording_source_buffer_size < n_samples) {
            throw std::runtime_error("Attempting to replace out of bounds of recording buffer");
        }
        auto data_length = ma_buffers_data_length.load();
        auto start_offset = ma_start_offset.load();
        auto data_position = (int)position + start_offset;
        bool changed = false;

        if (data_position < 0) {
            // skip ahead to the part that is in range
            const int skip = -data_position;
            const int n = (int)n_samples - skip;
            mp_recording_source_buffer += std::min(skip, (int)mp_recording_source_buffer_size);
            mp_recording_source_buffer_size = std::max((int)mp_recording_source_buffer_size - skip, 0);
            return PROC_process_replace(position + skip, length, std::max((int)n_samples - skip, 0));
        }

        if (mp_buffers.ensure_available(data_position + n_samples)) {
            data_length = ma_buffers_data_length;
            changed = true;
        }
        
        size_t samples_left = length - position;
        size_t buf_space = mp_buffers.buf_space_for_sample(data_position);
        SampleT* to = &mp_buffers.at(data_position);
        SampleT* &from = mp_recording_source_buffer;
        auto n = std::min({buf_space, samples_left, n_samples});
        auto rest = n_samples - n;

        // Queue the actual copy for later
        PROC_queue_memcpy((void*)to, (void*)from, sizeof(SampleT) * n);
        changed = changed || (n>0);

        mp_recording_source_buffer += n;
        mp_recording_source_buffer_size -= n;
        if (changed) { data_changed(); }

        // If we didn't replace all yet, go to next buffer and continue
        if(rest > 0) {
            PROC_process_replace(position + n, length, rest);
        }
    }

    size_t get_length() const override { return ma_buffers_data_length; }
    void PROC_set_length(size_t length) override { 
        ma_buffers_data_length = length;
        data_changed();
    }

    SampleT const& PROC_at(size_t position) const {
        return mp_buffers[position / ma_buffer_size]->at(position % mp_buffers.front()->size());
    }

    void PROC_process_playback(size_t position, size_t length, size_t n_samples, bool muted) {
        if (mp_playback_target_buffer_size < n_samples) {
            throw std::runtime_error("Attempting to play out of bounds of target buffer");
        }

        auto data_length = ma_buffers_data_length.load();
        auto start_offset = ma_start_offset.load();
        auto data_position = (int)position + start_offset;

        if (data_position < 0) {
            // skip ahead to the part that is in range
            const int skip = -data_position;
            position += skip;
            n_samples = std::max((int)n_samples - skip, 0);
            mp_recording_source_buffer += std::min(skip, (int)mp_recording_source_buffer_size);
            mp_recording_source_buffer_size = std::max((int)mp_recording_source_buffer_size - skip, 0);
        }
        
        if (data_position < data_length) {
            // We have something to play.
            size_t samples_left = length - position;
            size_t buf_space = mp_buffers.buf_space_for_sample(data_position);
            SampleT *from = &mp_buffers.at(data_position);
            SampleT* &to = mp_playback_target_buffer;
            auto n = std::min({buf_space, samples_left, n_samples});
            auto rest = n_samples - n;

            if (!muted) {
                PROC_queue_additivecpy(to, from, n, ma_volume, true);
            }

            mp_playback_target_buffer += n;
            mp_playback_target_buffer_size -= n;

            // If we didn't play back all yet, go to next buffer and continue
            if(rest > 0) {
                PROC_process_playback(position + n, length, rest, muted);
            }
        } else {
            // We have nothing to play. Just leave the output buffer as-is.
            mp_playback_target_buffer += n_samples;
            mp_playback_target_buffer_size -= n_samples;
        }
    }

    std::optional<size_t> PROC_get_next_poi(loop_mode_t mode,
                                       std::optional<loop_mode_t> maybe_next_mode,
                                       std::optional<size_t> maybe_next_mode_delay_cycles,
                                       std::optional<size_t> maybe_next_mode_eta,
                                       size_t length,
                                       size_t position) const override {
        std::optional<size_t> rval = std::nullopt;
        auto merge_poi = [&rval](size_t poi) {
            rval = rval.has_value() ? std::min(rval.value(), poi) : poi;
        };

        auto process_params = get_channel_process_params(
            mode,
            maybe_next_mode,
            maybe_next_mode_delay_cycles,
            maybe_next_mode_eta,
            position,
            ma_start_offset,
            ma_mode
        );

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

    void PROC_handle_poi(loop_mode_t mode,
                            size_t length,
                            size_t position) override {};


    void PROC_set_playback_buffer(SampleT *buffer, size_t size) {
        throw_if_commands_queued();
        mp_playback_target_buffer = buffer;
        mp_playback_target_buffer_size = size;
    }

    void PROC_set_recording_buffer(SampleT *buffer, size_t size) {
        throw_if_commands_queued();
        mp_recording_source_buffer = buffer;
        mp_recording_source_buffer_size = size;
    }

    void PROC_clear(size_t length) {
        throw_if_commands_queued();
        mp_buffers.ensure_available(length);
        ma_buffers_data_length = length;
        ma_start_offset = 0;
        data_changed();
    }

    float get_output_peak() const {
        return ma_output_peak;
    }

    void reset_output_peak() {
        ma_output_peak = 0.0f;
    }

    void set_mode(channel_mode_t mode) override {
        ma_mode = mode;
    }

    channel_mode_t get_mode() const override {
        return ma_mode;
    }

    void set_volume(float volume) {
        ma_volume = volume;
    }

    float get_volume() {
        return ma_volume;
    }
    
    void set_start_offset(int offset) override {
        ma_start_offset = offset;
    }

    int get_start_offset() const override {
        return ma_start_offset;
    }

    unsigned get_data_seq_nr() const override {
        return ma_data_seq_nr;
    }

protected:
    Buffer get_new_buffer() const {
        auto buf = Buffer(ma_buffer_pool->get_object());
        if (buf->size() != ma_buffer_size) {
            throw std::runtime_error("AudioChannel requires buffers of same length");
        }
        return buf;
    }
};