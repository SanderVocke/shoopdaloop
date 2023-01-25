#pragma once
#include "LoopInterface.h"
#include "AudioBufferPool.h"
#include "AudioBuffer.h"
#include <cstring>
#include <memory>
#include <stdexcept>
#include <vector>
#include <optional>

enum class AudioLoopOutputType {
    Copy,
    Add
};

template<typename SampleT>
class AudioLoop : public LoopInterface {
private:
    typedef AudioBufferPool<SampleT> BufferPool;
    typedef AudioBuffer<SampleT> Buffer;
    
    enum PointOfInterestTypeFlags {
        StateTransition   = 1, // The loop will transition to its next state.
        ExternalBufferEnd = 2, // The recording/playback buffer space runs out
        LoopEnd           = 4, // The loop ends
    };

    struct PointOfInterest {
        size_t when;
        unsigned type_flags;
    };

    struct Cursor {
        size_t position;       // Overall loop position
        size_t buffer_idx;     // idx of buffer in buffer list
        size_t pos_in_buffer;  // position within current buffer
    };

    std::optional<PointOfInterest> m_next_poi;
    std::shared_ptr<BufferPool> m_buffer_pool;
    std::vector<std::shared_ptr<Buffer>> m_buffers;
    loop_state_t m_state, m_next_state, m_next_next_state;
    size_t m_length;
    Cursor m_cursor;
    SampleT *m_playback_target_buffer;
    size_t   m_playback_target_buffer_size;
    SampleT *m_recording_source_buffer;
    size_t   m_recording_source_buffer_size;
    
    AudioLoopOutputType const m_output_type;

public:

    AudioLoop(std::shared_ptr<AudioBufferPool<SampleT>> buffer_pool, size_t initial_max_buffers, AudioLoopOutputType output_type) :
        m_buffer_pool(buffer_pool),
        m_length(0),
        m_cursor({.position = 0, .buffer_idx = 0, .pos_in_buffer = 0}),
        m_playback_target_buffer(nullptr),
        m_next_poi(std::nullopt),
        m_state(Stopped),
        m_next_state(Stopped),
        m_next_next_state(Stopped),
        m_playback_target_buffer_size(0),
        m_output_type(output_type)
    {
        m_buffers.reserve(initial_max_buffers);
        m_buffers.push_back(get_new_buffer()); // Initial recording buffer
    }

    AudioLoop() = default;

    ~AudioLoop() override {}

    std::optional<size_t> get_next_poi() const override {
        return m_next_poi ? m_next_poi.value().when : (std::optional<size_t>)std::nullopt;
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
        if(rest > 0) {
            m_cursor.buffer_idx++;
            m_buffers.push_back(std::shared_ptr<AudioBuffer<SampleT>>(m_buffer_pool->get_buffer()));
            m_cursor.pos_in_buffer = 0;
            process_record(rest);
        }
    }

    void process_playback(size_t n_samples, bool muted) {
        if (m_playback_target_buffer_size < n_samples) {
            throw std::runtime_error("Attempting to play out of bounds");
        }

        auto  &from_buf = m_buffers[m_cursor.buffer_idx];
        auto from = from_buf->data_at(m_cursor.pos_in_buffer);
        auto &to = m_playback_target_buffer;
        auto n = std::min(from_buf->head() - m_cursor.pos_in_buffer, n_samples);
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
            m_cursor.buffer_idx++;
            if(m_cursor.buffer_idx >= m_buffers.size()) {
                throw std::runtime_error("Playback reached end before expected");
            }
            m_cursor.pos_in_buffer = 0;
            process_playback(rest, muted);
        }
    }

    void process(size_t n_samples) override {
        if (m_next_poi && n_samples > m_next_poi.value().when) {
            throw std::runtime_error("Attempted to process loop beyond its next POI.");
        }

        switch(m_state) {
            case Recording:
                process_record(n_samples);
                break;
            case Playing:
                process_playback(n_samples, false);
                break;
            case PlayingMuted:
                process_playback(n_samples, true);
            case Stopped:
                break;
        }

        if (m_next_poi) { m_next_poi.value().when -= n_samples; }
    }

    void set_next_state(loop_state_t state) override { m_next_state = state; }
    void set_next_next_state(loop_state_t state) override { m_next_next_state = state; }
    loop_state_t get_state() const override { return m_state; }
    loop_state_t get_next_state() const override { return m_next_state; };
    loop_state_t get_next_next_state() const override { return m_next_next_state; };

    void plan_transition(size_t when=0) override {
        m_next_poi = dominant_poi(m_next_poi, PointOfInterest({.when = when, .type_flags = StateTransition}));
    }

    void transition_now() override {
        m_state = m_next_state;
        m_next_state = m_next_next_state;
        m_next_poi = std::nullopt;
        update_poi();
    }

    void update_poi() {
        // State transition POIs are managed via the external call interfaces.
        // Here we set the POIs that come from any other reason.
        std::optional<PointOfInterest> loop_end_poi, external_end_poi;
        if (m_state == Playing || m_state == PlayingMuted) {
            loop_end_poi = {.when = m_length - m_cursor.position, .type_flags = LoopEnd};
            external_end_poi = {.when = m_playback_target_buffer_size, .type_flags = ExternalBufferEnd};
        } else if (m_state == Recording) {
            external_end_poi = {.when = m_recording_source_buffer_size, .type_flags = ExternalBufferEnd};
        }

        m_next_poi = dominant_poi(m_next_poi, loop_end_poi, external_end_poi);
    }

    void set_playback_buffer(SampleT *buffer, size_t size) {
        m_playback_target_buffer = buffer;
        m_playback_target_buffer_size = size;
        if (m_state == Playing || m_state == PlayingMuted) {
            if (m_next_poi) { m_next_poi.type_flags &= ~(ExternalBufferEnd); }
            m_next_poi = dominant_poi(m_next_poi, {.when = m_playback_target_buffer_size, .type_flags = ExternalBufferEnd});
        }
    }

    void set_recording_buffer(SampleT *buffer, size_t size) {
        m_recording_source_buffer = buffer;
        m_recording_source_buffer_size = size;
        if (m_state == Recording) {
            if (m_next_poi) { m_next_poi.type_flags &= ~(ExternalBufferEnd); }
            m_next_poi = dominant_poi(m_next_poi, {.when = m_recording_source_buffer_size, .type_flags = ExternalBufferEnd});
        }
    }

    void handle_poi() override {
        if (!m_next_poi || m_next_poi.value().when != 0) {
            return;
        }

        auto type_flags = m_next_poi.value().type_flags;
        if (type_flags & StateTransition) {
            transition_now();
            type_flags &= ~(StateTransition);
        }
        if (type_flags & LoopEnd) {
            m_cursor = {.position = 0, .buffer_idx = 0, .pos_in_buffer = 0};
            type_flags &= ~(LoopEnd);
        }

        // Note: if any other type flags are given, we leave them there.
        // Those flags are meant to be cleared elsewhere.
        m_next_poi = std::nullopt;
        if (type_flags) {
            m_next_poi = PointOfInterest({ .when = 0, .type_flags = type_flags });
        }
        update_poi();
    }

    size_t get_position() const override { return m_cursor.position; }
    size_t get_length() const override { return m_length; }

protected:

    static std::optional<PointOfInterest> dominant_poi(std::optional<PointOfInterest> const& dummy) {
        return dummy;
    }

    static std::optional<PointOfInterest> dominant_poi(std::optional<PointOfInterest> const& a, std::optional<PointOfInterest> const& b) {
        if(a && !b) { return a; }
        if(b && !a) { return b; }
        if(!a && !b) { return std::nullopt; }
        if(a && b && a.value().when == b.value().when) {
            return PointOfInterest({.when = a.value().when, .type_flags = a.value().type_flags | b.value().type_flags });
        }
        if(a && b && a.value().when < b.value().when) {
            return a;
        }
        return b;
    }

    template<typename ...Args>
    static std::optional<PointOfInterest> dominant_poi(std::optional<PointOfInterest> const& first, Args... others) {
        return dominant_poi(first, dominant_poi(others...));
    }

    std::shared_ptr<Buffer> get_new_buffer() const {
        return std::shared_ptr<Buffer>(m_buffer_pool->get_buffer());
    }
};