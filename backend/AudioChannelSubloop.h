#pragma once
#include "SubloopInterface.h"
#include "ObjectPool.h"
#include "AudioBuffer.h"
#include "WithCommandQueue.h"
#include <cmath>
#include <cstring>
#include <memory>
#include <stdexcept>
#include <thread>
#include <vector>
#include <optional>
#include <iostream>
#include <chrono>

using namespace std::chrono_literals;

template<typename SampleT>
class AudioChannelSubloop : public SubloopInterface,
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

    // Members which may be accessed from the process thread only (mp prefix)
    std::vector<Buffer> mp_buffers;
    SampleT *mp_playback_target_buffer;
    size_t   mp_playback_target_buffer_size;
    SampleT *mp_recording_source_buffer;
    size_t   mp_recording_source_buffer_size;
public:

    AudioChannelSubloop(
            std::shared_ptr<BufferPool> buffer_pool,
            size_t initial_max_buffers) :
        WithCommandQueue<10, 1000, 1000>(),
        ma_buffer_pool(buffer_pool),
        ma_data_length(0),
        ma_buffer_size(buffer_pool->object_size()),
        mp_recording_source_buffer(nullptr),
        mp_playback_target_buffer(nullptr),
        mp_playback_target_buffer_size(0),
        mp_recording_source_buffer_size(0)
    {
        mp_buffers.reserve(initial_max_buffers);
        mp_buffers.push_back(get_new_buffer()); // Initial recording buffer

        // TODO
        std::cerr << "Warning: AudioChannelSubloop should have a way to increase its buffers capacity outside of the processing thread. Also atomic access. Also atomic planning of transitions." << std::endl;
    }

    // NOTE: only use on process thread!
    AudioChannelSubloop<SampleT>& operator= (AudioChannelSubloop<SampleT> const& other) {
        if (other.ma_buffer_size != ma_buffer_size) {
            throw std::runtime_error("Cannot copy audio channels with different buffer sizes.");
        }
        ma_buffer_pool = other.ma_buffer_pool;
        ma_data_length = other.ma_data_length.load();
        mp_buffers = other.mp_buffers;
        mp_playback_target_buffer = other.mp_playback_target_buffer;
        mp_playback_target_buffer_size = other.mp_playback_target_buffer_size;
        mp_recording_source_buffer = other.mp_recording_source_buffer;
        mp_recording_source_buffer_size = other.mp_recording_source_buffer_size;
        return *this;
    }

    AudioChannelSubloop() = default;
    ~AudioChannelSubloop() override {}

    void PROC_process(
        loop_mode_t mode_before,
        loop_mode_t mode_after,
        size_t n_samples,
        size_t pos_before,
        size_t pos_after,
        size_t length_before,
        size_t length_after
    ) override {
        // Execute any commands queued from other threads.
        PROC_handle_command_queue();

        // FIXME: always passthrough at full volume
        if (mp_playback_target_buffer_size >= n_samples && mp_recording_source_buffer_size >= n_samples) {
            memcpy((void*)mp_playback_target_buffer, (void*)mp_recording_source_buffer, sizeof(SampleT) * n_samples);
        }

        switch (mode_before) {
            case Playing:
                PROC_process_playback(pos_before, length_before, n_samples, false);
                break;
            case Recording:
                PROC_process_record(n_samples, length_before);
                break;
            default:
                break;
        }
    }

    // Load data into the loop. Should always be called outside
    // the processing thread.
    void load_data(SampleT* samples, size_t len, bool thread_safe = true) {
        // Convert to internal storage layout
        std::vector<Buffer> buffers(std::ceil((float)len / (float)ma_buffer_size));
        for (size_t idx=0; idx < buffers.size(); idx++) {
            auto &buf = buffers[idx];
            buf = std::make_shared<AudioBuffer<SampleT>>(ma_buffer_size);
            size_t already_copied = idx*ma_buffer_size;
            size_t n_elems = std::min(ma_buffer_size, len - already_copied);
            memcpy(buf->data(), samples + (idx * ma_buffer_size), sizeof(SampleT)*n_elems);
        }

        auto cmd = [&]() {
            mp_buffers = buffers;
            ma_data_length = len;
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
        std::vector<Buffer> buffers;
        size_t length;
        auto cmd = [&]() {
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
            throw std::runtime_error("Attempting to record out of bounds");
        }

        auto &from = mp_recording_source_buffer;
        auto buf_idx = ma_data_length / ma_buffer_size;
        auto buf_head = ma_data_length % ma_buffer_size;
        auto buf_space = ma_buffer_size - buf_head;
        while (mp_buffers.size() <= buf_idx) {
            mp_buffers.push_back(get_new_buffer());
        }
        auto &to_buf = mp_buffers[buf_idx];
        
        // Record all, or to the end of the current buffer, whichever
        // comes first
        auto n = std::min(n_samples, buf_space);
        auto rest = n_samples - n;
        memcpy((void*) &to_buf->at(buf_head), (void*)from, sizeof(SampleT)*n);

        mp_recording_source_buffer += n;
        mp_recording_source_buffer_size -= n;
        ma_data_length += n;

        // If we reached the end, add another buffer
        // and record the rest.
        size_t length_before_next = length_before + n;
        if(rest > 0) {
            PROC_process_record(rest, length_before_next);
        }
    }

    size_t data_length() const { return ma_data_length; }

    SampleT const& PROC_at(size_t position) const {
        return mp_buffers[position / ma_buffer_size]->at(position % mp_buffers.front()->size());
    }

    void PROC_process_playback(size_t position, size_t length, size_t n_samples, bool muted) {
        if (mp_playback_target_buffer_size < n_samples) {
            throw std::runtime_error("Attempting to play out of bounds of target buffer");
        }
        if (ma_data_length == 0) {
            throw std::runtime_error("Attempting to play from empty channel");
        }

        size_t buffer_idx = position / ma_buffer_size;
        size_t pos_in_buffer = position % ma_buffer_size;
        size_t buf_head = (buffer_idx == mp_buffers.size()-1) ?
            ma_data_length - (buffer_idx * ma_buffer_size) :
            ma_buffer_size;
        auto  &from_buf = mp_buffers[buffer_idx];
        SampleT* from = &from_buf->at(pos_in_buffer);
        SampleT* &to = mp_playback_target_buffer;
        auto n = std::min(buf_head - pos_in_buffer, n_samples);
        auto rest = n_samples - n;

        if (!muted) {
            for(size_t idx=0; idx < n; idx++) {
                to[idx] += from[idx];
            }
        }

        mp_playback_target_buffer += n;
        mp_playback_target_buffer_size -= n;

        // If we didn't play back all yet, go to next buffer and continue
        if(rest > 0) {
            PROC_process_playback(position + n, length, rest, muted);
        }
    }

    std::optional<size_t> PROC_get_next_poi(loop_mode_t mode,
                                       size_t length,
                                       size_t position) const override {
        if (mode == Playing) {
            return mp_playback_target_buffer_size;
        } else if (mode == Recording) {
            return mp_recording_source_buffer_size;
        }
        return std::nullopt;
    }

    void PROC_handle_poi(loop_mode_t mode,
                            size_t length,
                            size_t position) override {};


    void PROC_set_playback_buffer(SampleT *buffer, size_t size) {
        mp_playback_target_buffer = buffer;
        mp_playback_target_buffer_size = size;
    }

    void PROC_set_recording_buffer(SampleT *buffer, size_t size) {
        mp_recording_source_buffer = buffer;
        mp_recording_source_buffer_size = size;
    }

    void PROC_clear(size_t length) {
        mp_buffers.resize(std::ceil((float)length / (float)ma_buffer_size));
        for (auto &b: mp_buffers) {
            memset((void*)b->data(), 0, sizeof(SampleT) * b->size());
        }
        ma_data_length = length;
    }

protected:
    Buffer get_new_buffer() const {
        auto buf = Buffer(ma_buffer_pool->get_object());
        if (buf->size() != ma_buffer_size) {
            throw std::runtime_error("AudioLoop requires buffers of same length");
        }
        return buf;
    }
};