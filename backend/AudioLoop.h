#pragma once
#include "BasicLoop.h"
#include "AudioBufferPool.h"
#include "AudioBuffer.h"
#include "AudioLoopTestInterface.h"
#include <cstring>
#include <memory>
#include <stdexcept>
#include <vector>
#include <optional>
#include <iostream>

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
        m_buffer_size(buffer_pool->buffer_size())
    {
        m_buffers.reserve(initial_max_buffers);
        m_buffers.push_back(get_new_buffer()); // Initial recording buffer

        // TODO
        std::cerr << "Warning: AudioLoop should have a way to increase its buffers capacity outside of the processing thread. Also atomic access." << std::endl;
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

    void process_record(size_t n_samples) {
        if (m_recording_source_buffer_size < n_samples) {
            throw std::runtime_error("Attempting to record out of bounds");
        }

        auto &from = m_recording_source_buffer;
        auto &to_buf = m_buffers.back();
        auto to = to_buf->data_at(to_buf->head());
        // Record all, or to the end of the current buffer, whichever
        // comes first
        auto n = std::min(n_samples, to_buf->space());
        auto rest = n_samples - n;
        to_buf->record(from, n);

        m_recording_source_buffer += n;
        m_recording_source_buffer_size -= n;

        // If we reached the end of the buffer, add another
        // and record the rest.
        if(m_buffers.back()->space() == 0) {
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
        auto  &from_buf = m_buffers[buffer_idx];
        auto from = from_buf->data_at(pos_in_buffer);
        auto &to = m_playback_target_buffer;
        auto n = std::min(from_buf->head() - pos_in_buffer, n_samples);
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
            m_next_poi = dominant_poi(m_next_poi, {.when = m_playback_target_buffer_size, .type_flags = ExternalBufferEnd});
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