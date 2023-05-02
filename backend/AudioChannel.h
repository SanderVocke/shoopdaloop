#pragma once
#include "ChannelInterface.h"
#include "ObjectPool.h"
#include "AudioBuffer.h"
#include "WithCommandQueue.h"
#include "types.h"
#include "modified_loop_mode_for_channel.h"
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
    // Members which may be accessed from any thread (ma prefix)
    std::shared_ptr<BufferPool> ma_buffer_pool;
    const size_t ma_buffer_size;
    std::atomic<size_t> ma_data_length;
    std::atomic<int> ma_start_offset;
    std::atomic<float> ma_output_peak;
    std::atomic<float> ma_volume;
    std::atomic<channel_mode_t> ma_mode;

    // Members which may be accessed from the process thread only (mp prefix)
    std::vector<Buffer> mp_buffers;
    SampleT *mp_playback_target_buffer;
    size_t   mp_playback_target_buffer_size;
    SampleT *mp_recording_source_buffer;
    size_t   mp_recording_source_buffer_size;

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
        ma_data_length(0),
        ma_buffer_size(buffer_pool->object_size()),
        mp_recording_source_buffer(nullptr),
        mp_playback_target_buffer(nullptr),
        mp_playback_target_buffer_size(0),
        mp_recording_source_buffer_size(0),
        ma_output_peak(0),
        ma_mode(mode),
        ma_volume(1.0f),
        ma_start_offset(0)
    {
        mp_buffers.reserve(initial_max_buffers);
        mp_buffers.push_back(get_new_buffer()); // Initial recording buffer
    }

    // NOTE: only use on process thread!
    AudioChannel<SampleT>& operator= (AudioChannel<SampleT> const& other) {
        throw_if_commands_queued();
        if (other.ma_buffer_size != ma_buffer_size) {
            throw std::runtime_error("Cannot copy audio channels with different buffer sizes.");
        }
        ma_buffer_pool = other.ma_buffer_pool;
        ma_data_length = other.ma_data_length.load();
        ma_start_offset = other.ma_start_offset.load();
        mp_buffers = other.mp_buffers;
        mp_playback_target_buffer = other.mp_playback_target_buffer;
        mp_playback_target_buffer_size = other.mp_playback_target_buffer_size;
        mp_recording_source_buffer = other.mp_recording_source_buffer;
        mp_recording_source_buffer_size = other.mp_recording_source_buffer_size;
        ma_mode = other.ma_mode;
        ma_volume = other.ma_volume;
        mp_proc_queue.reset();
        return *this;
    }

    AudioChannel() = default;
    ~AudioChannel() override {}

    void PROC_process(
        loop_mode_t mode,
        size_t n_samples,
        size_t pos_before,
        size_t pos_after,
        size_t length_before,
        size_t length_after
    ) override {
        // Execute any commands queued from other threads.
        PROC_handle_command_queue();

        loop_mode_t modified_mode = modified_loop_mode_for_channel(mode, ma_mode);

        switch (modified_mode) {
            case Playing:
                PROC_process_playback(pos_before, length_before, n_samples, false);
                break;
            case Recording:
                PROC_process_record(n_samples, length_before);
                break;
            case Replacing:
                PROC_process_replace(pos_before, length_before, n_samples);
                break;
            case Stopped:
                break;
            default:
                throw std::runtime_error("Unhandled loop mode in channel");
                break;
        }
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
        throw_if_commands_queued();

        // Convert to internal storage layout
        auto buffers = std::make_shared<std::vector<Buffer>>(std::ceil((float)len / (float)ma_buffer_size));
        for (size_t idx=0; idx < buffers->size(); idx++) {
            auto &buf = buffers->at(idx);
            buf = std::make_shared<AudioBuffer<SampleT>>(ma_buffer_size);
            size_t already_copied = idx*ma_buffer_size;
            size_t n_elems = std::min(ma_buffer_size, len - already_copied);
            memcpy(buf->data(), samples + (idx * ma_buffer_size), sizeof(SampleT)*n_elems);
        }

        auto cmd = [=]() {
            mp_buffers = *buffers;
            ma_data_length = len;
            ma_start_offset = 0;
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
        throw_if_commands_queued();

        std::vector<Buffer> buffers;
        size_t length;
        auto cmd = [this, &buffers, &length]() {
            buffers = mp_buffers;
            length = ma_data_length;
        };

        if (thread_safe) {
            exec_process_thread_command(cmd);
        } else {
            cmd();
        }

        std::vector<SampleT> rval(length);
        for(size_t idx=0; idx<length; idx++) {
            rval[idx] = buffers.at(idx / ma_buffer_size)->at(idx % ma_buffer_size);
        }
        return rval;
    }

    void PROC_process_record(size_t n_samples, size_t length_before) {
        if (mp_recording_source_buffer_size < n_samples) {
            throw std::runtime_error("Attempting to record out of bounds of input buffer");
        }
        auto start_offset = ma_start_offset.load();
        auto data_length = ma_data_length.load();

        auto &from = mp_recording_source_buffer;
        auto buf_idx = std::max(((int)length_before + start_offset), 0) / ma_buffer_size;
        auto buf_head = std::max(((int)length_before + start_offset), 0) % ma_buffer_size;
        auto buf_space = ma_buffer_size - buf_head;
        while (mp_buffers.size() <= buf_idx) {
            mp_buffers.push_back(get_new_buffer());
        }
        auto &to_buf = mp_buffers[buf_idx];
        
        // Record all, or to the end of the current buffer, whichever
        // comes first
        auto n = std::min(n_samples, buf_space);
        auto rest = n_samples - n;

        // Queue the actual copy for later.
        PROC_queue_memcpy((void*) &to_buf->at(buf_head), (void*)from, sizeof(SampleT)*n);

        mp_recording_source_buffer += n;
        mp_recording_source_buffer_size -= n;
        ma_data_length = std::max(data_length, length_before + start_offset + n);

        // If we reached the end, add another buffer
        // and record the rest.
        size_t length_before_next = length_before + n;
        if(rest > 0) {
            PROC_process_record(rest, length_before_next);
        }
    }

    void PROC_process_replace(size_t position, size_t length, size_t n_samples) {
        if (mp_recording_source_buffer_size < n_samples) {
            throw std::runtime_error("Attempting to replace out of bounds of recording buffer");
        }
        auto data_length = ma_data_length.load();
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

        while (data_length < (data_position + n_samples)) {
            // In replacement, we lengthen the buffer as needed.
            mp_buffers.push_back(get_new_buffer());
            ma_data_length += mp_buffers.back()->size();
            data_length = ma_data_length;
        }

        size_t buffer_idx = data_position / ma_buffer_size;
        size_t pos_in_buffer = data_position % ma_buffer_size;
        size_t buf_head = (buffer_idx == mp_buffers.size()-1) ?
            data_length - (buffer_idx * ma_buffer_size) :
            ma_buffer_size;
        size_t samples_left = length - position;
        auto  &to_buf = mp_buffers[buffer_idx];
        SampleT* to = &to_buf->at(pos_in_buffer);
        SampleT* &from = mp_recording_source_buffer;
        auto n = std::min({buf_head - pos_in_buffer, samples_left, n_samples});
        auto rest = n_samples - n;

        // Queue the actual copy for later
        PROC_queue_memcpy((void*)to, (void*)from, sizeof(SampleT) * n);

        mp_recording_source_buffer += n;
        mp_recording_source_buffer_size -= n;

        // If we didn't replace all yet, go to next buffer and continue
        if(rest > 0) {
            PROC_process_replace(position + n, length, rest);
        }
    }

    size_t get_length() const override { return ma_data_length; }
    void PROC_set_length(size_t length) override { ma_data_length = length; }

    SampleT const& PROC_at(size_t position) const {
        return mp_buffers[position / ma_buffer_size]->at(position % mp_buffers.front()->size());
    }

    void PROC_process_playback(size_t position, size_t length, size_t n_samples, bool muted) {
        if (mp_playback_target_buffer_size < n_samples) {
            throw std::runtime_error("Attempting to play out of bounds of target buffer");
        }

        auto data_length = ma_data_length.load();
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
            size_t buffer_idx = data_position / ma_buffer_size;
            size_t pos_in_buffer = data_position % ma_buffer_size;
            size_t buf_head = (buffer_idx == mp_buffers.size()-1) ?
                data_length - (buffer_idx * ma_buffer_size) :
                ma_buffer_size;
            size_t samples_left = length - position;
            auto  &from_buf = mp_buffers[buffer_idx];
            SampleT* from = &from_buf->at(pos_in_buffer);
            SampleT* &to = mp_playback_target_buffer;
            auto n = std::min({buf_head - pos_in_buffer, samples_left, n_samples});
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
                                       size_t length,
                                       size_t position) const override {
        if (ma_mode) {
            if (mode == Playing) {
                return mp_playback_target_buffer_size;
            } else if (mode == Recording || mode == Replacing) {
                return mp_recording_source_buffer_size;
            }
        }
        return std::nullopt;
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
        mp_buffers.resize(std::ceil((float)length / (float)ma_buffer_size));
        for (auto &b: mp_buffers) {
            memset((void*)b->data(), 0, sizeof(SampleT) * b->size());
        }
        ma_data_length = length;
        ma_start_offset = 0;
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

protected:
    Buffer get_new_buffer() const {
        auto buf = Buffer(ma_buffer_pool->get_object());
        if (buf->size() != ma_buffer_size) {
            throw std::runtime_error("AudioChannel requires buffers of same length");
        }
        return buf;
    }
};