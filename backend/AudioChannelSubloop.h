#pragma once
#include "SubloopInterface.h"
#include "ObjectPool.h"
#include "AudioBuffer.h"
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

enum class AudioOutputType {
    Copy,
    Add
};

template<typename SampleT>
class AudioChannelSubloop : public SubloopInterface {
public:
    typedef AudioBuffer<SampleT> BufferObj;
    typedef ObjectPool<BufferObj> BufferPool;
    typedef std::shared_ptr<BufferObj> Buffer;
    
private:
    std::shared_ptr<BufferPool> m_buffer_pool;
    std::vector<Buffer> m_buffers;
    SampleT *m_playback_target_buffer;
    size_t   m_playback_target_buffer_size;
    SampleT *m_recording_source_buffer;
    size_t   m_recording_source_buffer_size;
    AudioOutputType const m_output_type;
    size_t const m_buffer_size;

    std::atomic<size_t> m_data_length;

    // For copying data in/out. Buffer pointers are copied during the process
    // function to avoid race conditions, data is copied in other threads
    // for performance.
    struct DataSnapshot {
        size_t length;
        std::vector<Buffer> *data;
    };
    std::atomic<DataSnapshot *> maybe_copy_data_to;
    std::atomic<DataSnapshot *> maybe_copy_data_from;

public:

    AudioChannelSubloop(
            std::shared_ptr<BufferPool> buffer_pool,
            size_t initial_max_buffers,
            AudioOutputType output_type) :
        m_buffer_pool(buffer_pool),
        m_playback_target_buffer(nullptr),
        m_playback_target_buffer_size(0),
        m_recording_source_buffer_size(0),
        m_data_length(0),
        m_output_type(output_type),
        m_buffer_size(buffer_pool->object_size()),
        maybe_copy_data_from(nullptr),
        maybe_copy_data_to(nullptr)
    {
        m_buffers.reserve(initial_max_buffers);
        m_buffers.push_back(get_new_buffer()); // Initial recording buffer

        // TODO
        std::cerr << "Warning: AudioChannelSubloop should have a way to increase its buffers capacity outside of the processing thread. Also atomic access. Also atomic planning of transitions." << std::endl;
    }

    AudioChannelSubloop() = default;
    ~AudioChannelSubloop() override {}

    void process(
        loop_state_t state_before,
        loop_state_t state_after,
        size_t n_samples,
        size_t pos_before,
        size_t pos_after,
        size_t length_before,
        size_t length_after
    ) override {
        // Copy data pointers and length in/out safely.
        if (maybe_copy_data_to)     { 
            *(maybe_copy_data_to.load())->data = m_buffers;
            (maybe_copy_data_to.load())->length = m_data_length;
            maybe_copy_data_to = nullptr;
        }
        if (maybe_copy_data_from) {
            m_buffers = *(maybe_copy_data_from.load())->data;
            m_data_length = maybe_copy_data_from.load()->length;
            maybe_copy_data_from = nullptr;
        }

        switch (state_before) {
            case Playing:
            case PlayingMuted:
                process_playback(pos_before, length_before, n_samples, state_before == PlayingMuted);
                break;
            case Recording:
                process_record(n_samples, length_before);
                break;
            default:
                break;
        }
    }

    // Load data into the loop. Should always be called outside
    // the processing thread.
    void load_data(SampleT* samples, size_t len, bool thread_safe = true) {
        static constexpr auto poll_interval = 10ms;

        // Convert to internal storage layout
        std::vector<Buffer> buffers(std::ceil((float)len / (float)m_buffer_size));
        for (size_t idx=0; idx < buffers.size(); idx++) {
            auto &buf = buffers[idx];
            buf = std::make_shared<AudioBuffer<SampleT>>(m_buffer_size);
            size_t n_elems = idx == (buffers.size() - 1) && len < (m_buffers.size() * m_buffer_size) ?
                len % m_buffer_size : m_buffer_size;
            memcpy(buf->data(), samples + (idx * m_buffer_size), sizeof(SampleT)*n_elems);
        }
        DataSnapshot snapshot {len, &buffers};

        if (thread_safe) {
            // Copy in during process
            maybe_copy_data_from = &snapshot;
            std::cerr << "Warning: no timeout mechanism implemented" << std::endl;
            while (maybe_copy_data_from != nullptr) {
                std::this_thread::sleep_for(poll_interval);
            }
        } else {
            m_buffers = *snapshot.data;
            m_data_length = snapshot.length;
        }
    }

    // Get loop data. Should always be called outside
    // the processing thread.
    std::vector<SampleT> get_data(bool thread_safe = true) {
        static constexpr auto poll_interval = 10ms;

        std::vector<Buffer> buffers;
        DataSnapshot snapshot { 0, &buffers };

        if (thread_safe) {
            // Copy out during process
            maybe_copy_data_to = &snapshot;
            std::cerr << "Warning: no timeout mechanism implemented" << std::endl;
            while (maybe_copy_data_to != nullptr) {
                std::this_thread::sleep_for(poll_interval);
            }
        } else {
            *snapshot.data = m_buffers;
            snapshot.length = m_data_length;
        }

        std::vector<SampleT> samples(snapshot.length);
        for(size_t idx=0; idx < samples.size(); idx++) {
            samples[idx] = buffers[idx / m_buffer_size]->at(idx % m_buffer_size);
        }

        return samples;
    }

    void process_record(size_t n_samples, size_t length_before) {
        if (m_recording_source_buffer_size < n_samples) {
            throw std::runtime_error("Attempting to record out of bounds");
        }

        auto &from = m_recording_source_buffer;
        auto buf_idx = m_data_length / m_buffer_size;
        auto buf_head = m_data_length % m_buffer_size;
        auto buf_space = m_buffer_size - buf_head;
        while (m_buffers.size() <= buf_idx) {
            m_buffers.push_back(get_new_buffer());
        }
        auto &to_buf = m_buffers[buf_idx];
        
        // Record all, or to the end of the current buffer, whichever
        // comes first
        auto n = std::min(n_samples, buf_space);
        auto rest = n_samples - n;
        memcpy((void*) &to_buf->at(buf_head), (void*)from, sizeof(SampleT)*n);

        m_recording_source_buffer += n;
        m_recording_source_buffer_size -= n;
        m_data_length += n;

        // If we reached the end, add another buffer
        // and record the rest.
        size_t length_before_next = length_before + n;
        if(rest > 0) {
            process_record(rest, length_before_next);
        }
    }

    SampleT const& at(size_t pos) const {
        return m_buffers[pos / m_buffer_size]->at(pos % m_buffers.front()->size());
    }

    size_t data_length() const { return m_data_length; }

    void process_playback(size_t pos, size_t length, size_t n_samples, bool muted) {
        if (m_playback_target_buffer_size < n_samples) {
            throw std::runtime_error("Attempting to play out of bounds");
        }

        size_t buffer_idx = pos / m_buffer_size;
        size_t pos_in_buffer = pos % m_buffer_size;
        size_t buf_head = buffer_idx == m_buffers.size()-1 && length < (m_buffer_size * m_buffers.size()) ?
            length % m_buffer_size :
            m_buffer_size;
        auto  &from_buf = m_buffers[buffer_idx];
        SampleT* from = &from_buf->at(pos_in_buffer);
        SampleT* &to = m_playback_target_buffer;
        auto n = std::min(buf_head - pos_in_buffer, n_samples);
        auto rest = n_samples - n;

        if (!muted) {
            if (m_output_type == AudioOutputType::Copy) {
                memcpy((void*)to, (void*)from, n*sizeof(SampleT));
            } else if (m_output_type == AudioOutputType::Add) {
                for(size_t idx=0; idx < n; idx++) {
                    to[idx] += from[idx];
                }
            } else {
                throw std::runtime_error("Unsupported output type for audio loop.");
            }
        }

        m_playback_target_buffer += n;
        m_playback_target_buffer_size -= n;

        // If we didn't play back all yet, go to next buffer and continue
        if(rest > 0) {
            process_playback(pos + n, length, rest, muted);
        }
    }

    std::optional<size_t> get_next_poi(loop_state_t state,
                                       size_t length,
                                       size_t position) const override {
        if (state == Playing || state == PlayingMuted) {
            return m_playback_target_buffer_size;
        } else if (state == Recording) {
            return m_recording_source_buffer_size;
        }
        return std::nullopt;
    }

    void handle_poi(loop_state_t state,
                            size_t length,
                            size_t position) override {};


    void set_playback_buffer(SampleT *buffer, size_t size) {
        m_playback_target_buffer = buffer;
        m_playback_target_buffer_size = size;
    }

    void set_recording_buffer(SampleT *buffer, size_t size) {
        m_recording_source_buffer = buffer;
        m_recording_source_buffer_size = size;
    }

protected:
    Buffer get_new_buffer() const {
        auto buf = Buffer(m_buffer_pool->get_object());
        if (m_buffers.size() > 0 && buf->size() != m_buffers.back()->size()) {
            throw std::runtime_error("AudioLoop requires buffers of same length");
        }
        return buf;
    }
};