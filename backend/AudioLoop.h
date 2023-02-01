#pragma once
#include "BasicLoop.h"
#include "AudioBufferPool.h"
#include "AudioBuffer.h"
#include "AudioLoopTestInterface.h"
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

enum class AudioLoopOutputType {
    Copy,
    Add
};

template<typename SampleT>
class AudioLoop : public BasicLoop,
                  public AudioLoopTestInterface<std::shared_ptr<AudioBuffer<SampleT>>> {
public:
    typedef AudioBufferPool<SampleT> BufferPool;
    typedef AudioBuffer<SampleT> BufferObj;
    typedef std::shared_ptr<BufferObj> Buffer;
    
private:
    enum AudioLoopPointOfInterestTypeFlags {
        ExternalBufferEnd = BasicPointOfInterestFlags_End,
    };

    std::shared_ptr<BufferPool> m_buffer_pool;
    std::vector<Buffer> m_buffers;
    SampleT *m_playback_target_buffer;
    size_t   m_playback_target_buffer_size;
    SampleT *m_recording_source_buffer;
    size_t   m_recording_source_buffer_size;
    AudioLoopOutputType const m_output_type;
    size_t const m_buffer_size;

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

    AudioLoop(
            std::shared_ptr<AudioBufferPool<SampleT>> buffer_pool,
            size_t initial_max_buffers,
            AudioLoopOutputType output_type) :
        BasicLoop(),
        m_buffer_pool(buffer_pool),
        m_playback_target_buffer(nullptr),
        m_playback_target_buffer_size(0),
        m_recording_source_buffer_size(0),
        m_output_type(output_type),
        m_buffer_size(buffer_pool->buffer_size()),
        maybe_copy_data_from(nullptr),
        maybe_copy_data_to(nullptr)
    {
        m_buffers.reserve(initial_max_buffers);
        m_buffers.push_back(get_new_buffer()); // Initial recording buffer

        // TODO
        std::cerr << "Warning: AudioLoop should have a way to increase its buffers capacity outside of the processing thread. Also atomic access. Also atomic planning of transitions." << std::endl;
    }

    AudioLoop() = default;

    ~AudioLoop() override {}

    void process_body(
        size_t n_samples,
        size_t pos_before,
        size_t &pos_after,
        size_t length_before,
        size_t &length_after,
        loop_state_t state_before,
        loop_state_t &state_after
        ) override {
        // Copy data pointers and length in/out safely.
        if (maybe_copy_data_to)     { 
            *(maybe_copy_data_to.load())->data = m_buffers;
            (maybe_copy_data_to.load())->length = get_length();
            maybe_copy_data_to = nullptr;
        }
        if (maybe_copy_data_from) {
            m_buffers = *(maybe_copy_data_from.load())->data;
            set_length((maybe_copy_data_from.load())->length);
            length_after = get_length(); // TODO: not nice that we have to do this. Otherwise BasicLoop overwrites our length change
            maybe_copy_data_from = nullptr;
        }

        switch (get_state()) {
            case Playing:
            case PlayingMuted:
                process_playback(pos_before, n_samples, get_state() == PlayingMuted);
            case Recording:
                process_record(n_samples);
            default:
                break;
        }
    }

    // Load data into the loop. Should always be called outside
    // the processing thread.
    void load_data(SampleT* samples, size_t len) {
        static constexpr auto poll_interval = 10ms;

        // Convert to internal storage layout
        std::vector<Buffer> buffers(std::ceil((float)len / (float)m_buffer_size));
        for (size_t idx=0; idx < buffers.size(); idx++) {
            auto &buf = buffers[idx];
            buf = std::make_shared<AudioBuffer<float>>(m_buffer_size);
            size_t n_elems = idx >= (buffers.size() - 1) ?
                len % m_buffer_size : m_buffer_size;
            memcpy(buf->data(), samples + (idx * m_buffer_size), sizeof(SampleT)*n_elems);
        }
        DataSnapshot snapshot {len, &buffers};

        // Copy in during process
        maybe_copy_data_from = &snapshot;
        while (maybe_copy_data_from != nullptr) {
            std::this_thread::sleep_for(poll_interval);
        }
    }

    // Get loop data. Should always be called outside
    // the processing thread.
    std::vector<SampleT> get_data() {
        static constexpr auto poll_interval = 10ms;

        std::vector<Buffer> buffers;
        DataSnapshot snapshot { 0, &buffers };

        // Copy out during process
        maybe_copy_data_to = &snapshot;
        while (maybe_copy_data_to != nullptr) {
            std::this_thread::sleep_for(poll_interval);
        }

        std::vector<SampleT> samples(snapshot.length);
        for(size_t idx=0; idx < samples.size(); idx++) {
            samples[idx] = buffers[idx / m_buffer_size]->at(idx % m_buffer_size);
        }

        return samples;
    }

    void process_record(size_t n_samples) {
        if (m_recording_source_buffer_size < n_samples) {
            throw std::runtime_error("Attempting to record out of bounds");
        }

        auto &from = m_recording_source_buffer;
        auto &to_buf = m_buffers.back();
        auto buf_head = m_length % m_buffer_size;
        auto buf_space = m_buffer_size - buf_head;
        
        // Record all, or to the end of the current buffer, whichever
        // comes first
        auto n = std::min(n_samples, buf_space);
        auto rest = n_samples - n;
        memcpy((void*) &to_buf->at(buf_head), (void*)from, sizeof(SampleT)*n);

        m_recording_source_buffer += n;
        m_recording_source_buffer_size -= n;

        // If we reached the end, add another buffer
        // and record the rest.
        if(m_length >= (m_buffer_size * m_buffers.size())) {
            m_buffers.push_back(get_new_buffer());
        }
        if(rest > 0) {
            process_record(rest);
        }
    }

    SampleT const& at(size_t pos) const {
        return m_buffers[pos / m_buffers.front()->size()]->at(pos % m_buffers.front()->size());
    }

    void process_playback(size_t pos, size_t n_samples, bool muted) {
        if (m_playback_target_buffer_size < n_samples) {
            throw std::runtime_error("Attempting to play out of bounds");
        }

        size_t buffer_idx = pos / m_buffer_size;
        size_t pos_in_buffer = pos % m_buffer_size;
        size_t buf_head = buffer_idx == m_buffers.size()-1 ?
            m_length % m_buffer_size :
            m_buffer_size;
        auto  &from_buf = m_buffers[buffer_idx];
        SampleT* from = &from_buf->at(pos_in_buffer);
        SampleT* &to = m_playback_target_buffer;
        auto n = std::min(buf_head - pos_in_buffer, n_samples);
        auto rest = n_samples - n;

        if (m_output_type == AudioLoopOutputType::Copy) {
            memcpy((void*)to, (void*)from, n*sizeof(SampleT));
        } else if (m_output_type == AudioLoopOutputType::Add) {
            for(size_t idx=0; idx < n; idx++) {
                to[idx] += from[idx];
            }
        } else {
            throw std::runtime_error("Unsupported output type for audio loop.");
        }

        m_playback_target_buffer += n;
        m_playback_target_buffer_size -= n;

        // If we didn't play back all yet, go to next buffer and continue
        if(rest > 0) {
            process_playback(pos + n, rest, muted);
        }
    }

    void update_poi() override {
        BasicLoop::update_poi();

        // Handle our own POI types
        std::optional<PointOfInterest> external_end_poi;
        if (get_state() == Playing || get_state() == PlayingMuted) {
            external_end_poi = {.when = m_playback_target_buffer_size, .type_flags = ExternalBufferEnd};
        } else if (get_state() == Recording) {
            external_end_poi = {.when = m_recording_source_buffer_size, .type_flags = ExternalBufferEnd};
        }

        m_next_poi = BasicLoop::dominant_poi(m_next_poi, external_end_poi);
    }


    void set_playback_buffer(SampleT *buffer, size_t size) {
        m_playback_target_buffer = buffer;
        m_playback_target_buffer_size = size;
        if (m_state == Playing || m_state == PlayingMuted) {
            if (m_next_poi) { m_next_poi->type_flags &= ~(ExternalBufferEnd); }
            m_next_poi = dominant_poi(m_next_poi, PointOfInterest{.when = m_playback_target_buffer_size, .type_flags = ExternalBufferEnd});
        }
    }

    void set_recording_buffer(SampleT *buffer, size_t size) {
        m_recording_source_buffer = buffer;
        m_recording_source_buffer_size = size;
        if (m_state == Recording) {
            if (m_next_poi) { m_next_poi->type_flags &= ~(ExternalBufferEnd); }
            m_next_poi = dominant_poi(m_next_poi, PointOfInterest{.when = m_recording_source_buffer_size, .type_flags = ExternalBufferEnd});
        }
    }

protected:
    Buffer get_new_buffer() const {
        auto buf = Buffer(m_buffer_pool->get_buffer());
        if (m_buffers.size() > 0 && buf->size() != m_buffers.back()->size()) {
            throw std::runtime_error("AudioLoop requires buffers of same length");
        }
        return buf;
    }

    // Testing interfaces
    std::vector<Buffer> &buffers() override { return m_buffers; }
    virtual size_t get_current_buffer_idx() const override { return get_position() / m_buffer_size; }
    virtual size_t get_position_in_current_buffer() const override { return get_position() % m_buffer_size; }
};