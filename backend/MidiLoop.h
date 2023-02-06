#pragma once
#include "BasicLoop.h"
#include "MidiStorage.h"
#include "MidiPortInterface.h"
#include <optional>
#include <functional>
#include <chrono>
#include <thread>

using namespace std::chrono_literals;

template<typename TimeType, typename SizeType>
class MidiLoop : public BasicLoop {
public:
    using Storage = MidiStorage<TimeType, SizeType>;
    using StorageCursor = typename Storage::Cursor;

private:
    enum MidiLoopPointOfInterestTypeFlags {
        ExternalBufferEnd = BasicPointOfInterestFlags_End,
    };

    template <typename Buf>
    struct ExternalBuf {
        Buf buf;
        size_t n_events_total;
        size_t n_frames_total;
        size_t n_events_processed;
        size_t n_frames_processed;

        ExternalBuf(Buf buf) : buf(buf),
            n_events_total(0),
            n_events_processed(0),
            n_frames_total(0),
            n_frames_processed(0) {}
        
        size_t frames_left() const { return n_frames_total - n_frames_processed; }
        size_t events_left() const { return n_events_total - n_events_processed; }
    };

    std::optional<ExternalBuf<MidiWriteableBufferInterface*>> m_playback_target_buffer;
    std::optional<ExternalBuf<MidiReadableBufferInterface *>> m_recording_source_buffer;
    std::shared_ptr<Storage>       m_storage;
    std::shared_ptr<StorageCursor> m_playback_cursor;

    // For copying data in/out. The copy takes place in the process thread
    // to avoid race conditions.
    struct DataSnapshot {
        size_t length;
        std::shared_ptr<Storage> data;
    };
    std::atomic<DataSnapshot *> maybe_copy_data_to;
    std::atomic<DataSnapshot *> maybe_copy_data_from;

public:
    MidiLoop(size_t data_size) :
        BasicLoop(),
        m_playback_target_buffer(nullptr),
        m_recording_source_buffer(nullptr),
        m_storage(std::make_shared<Storage>(data_size)),
        m_playback_cursor(nullptr),
        maybe_copy_data_from(nullptr),
        maybe_copy_data_to(nullptr)
    {
        m_playback_cursor = m_storage->create_cursor();
    }

    void process_body(
        size_t n_samples,
        size_t pos_before,
        size_t &pos_after,
        size_t length_before,
        size_t &length_after,
        loop_state_t state_before,
        loop_state_t &state_after
        ) override {
        if (maybe_copy_data_to) {
            m_storage->copy(*maybe_copy_data_to.load()->data);
            maybe_copy_data_to.load()->length = get_length();
            maybe_copy_data_to = nullptr;
        }
        if (maybe_copy_data_from) {
            m_storage = maybe_copy_data_from.load()->data;
            set_length(maybe_copy_data_from.load()->length);
            m_playback_cursor = m_storage->create_cursor();
            maybe_copy_data_from = nullptr;
        }

        switch (get_state()) {
            case Playing:
            case PlayingMuted:
                process_playback(pos_before, n_samples, get_state() == PlayingMuted);
                break;
            case Recording:
                process_record(length_before, n_samples);
                break;
            default:
                break;
        }
    }

    void process_record(size_t our_length, size_t n_samples) {
        if (!m_recording_source_buffer.has_value()) {
            throw std::runtime_error("Recording without source buffer");
        }
        auto &recbuf = m_recording_source_buffer.value();
        if (recbuf.frames_left() < n_samples) {
            throw std::runtime_error("Attempting to record out of bounds");
        }

        // Record any incoming events
        size_t record_end = recbuf.n_frames_processed + n_samples;
        for(size_t idx=recbuf.n_events_processed
            ;
            idx < recbuf.n_events_total
            ;
            idx++)
        {
            uint32_t t;
            uint32_t s;
            uint8_t *d;
            recbuf.buf->get_event(idx, s, t, d);
            if (t >= record_end) {
                break;
            }
            m_storage->append(our_length + (TimeType) t, (SizeType) s, d);
            recbuf.n_events_processed++;
        }
        
        recbuf.n_frames_processed += n_samples;
    }

    void clear() {
        m_storage->clear();
        m_playback_cursor.reset();
    }

    void process_playback(size_t our_pos, size_t n_samples, bool muted) {
        if (!m_playback_target_buffer.has_value()) {
            throw std::runtime_error("Playing without target buffer");
        }
        auto &buf = m_playback_target_buffer.value();
        if (buf.frames_left() < n_samples) {
            throw std::runtime_error("Attempting to play back out of bounds");
        }

        // Playback any events
        size_t end = buf.n_frames_processed + n_samples;
        m_playback_cursor->find_time_forward(our_pos);
        while(m_playback_cursor->valid())
        {
            auto *event = m_playback_cursor->get();
            size_t event_time = event->time - our_pos;
            if (event_time >= n_samples) {
                // Future event
                break;
            }
            if (!muted) {
                buf.buf->write_event(event->size, event_time, event->data);
            }
            buf.n_events_processed++;
            m_playback_cursor->next();
        }
        
        buf.n_frames_processed += n_samples;
    }

    void set_position(size_t pos) override {
        size_t old_pos = get_position();
        BasicLoop::set_position(pos);
        if(get_position() < old_pos) {
            m_playback_cursor.reset();
        }
    }

    void update_poi() override {
        BasicLoop::update_poi();

        // Handle our own POI types
        std::optional<PointOfInterest> external_end_poi;
        if (get_state() == Playing || get_state() == PlayingMuted) {
            size_t frames_left = m_playback_target_buffer.value().n_frames_total - m_playback_target_buffer.value().n_frames_processed;
            external_end_poi = {.when = frames_left, .type_flags = ExternalBufferEnd};
        } else if (get_state() == Recording) {
            size_t frames_left = m_recording_source_buffer.value().n_frames_total - m_recording_source_buffer.value().n_frames_processed;
            external_end_poi = {.when = frames_left, .type_flags = ExternalBufferEnd};
        }

        m_next_poi = BasicLoop::dominant_poi(m_next_poi, external_end_poi);
    }

    void set_playback_buffer(MidiWriteableBufferInterface *buffer, size_t n_frames) {
        m_playback_target_buffer = ExternalBuf<MidiWriteableBufferInterface *> (buffer);
        m_playback_target_buffer.value().n_frames_total = n_frames;
        if (m_state == Playing || m_state == PlayingMuted) {
            if (m_next_poi) { m_next_poi->type_flags &= ~(ExternalBufferEnd); }
            m_next_poi = dominant_poi(m_next_poi, PointOfInterest{.when = n_frames, .type_flags = ExternalBufferEnd});
        }
    }

    void set_recording_buffer(MidiReadableBufferInterface *buffer, size_t n_frames) {
        m_recording_source_buffer = ExternalBuf<MidiReadableBufferInterface *> (buffer);
        m_recording_source_buffer.value().n_frames_total = n_frames;
        m_recording_source_buffer.value().n_events_total = buffer->get_n_events();
        if (m_state == Recording) {
            if (m_next_poi) { m_next_poi->type_flags &= ~(ExternalBufferEnd); }
            m_next_poi = dominant_poi(m_next_poi, PointOfInterest{.when = n_frames, .type_flags = ExternalBufferEnd});
        }
    }

    void set_length(size_t length) override {
        size_t old_length = m_length;
        BasicLoop::set_length(length);
        if (m_length < old_length) {
            std::cerr << "Proper MIDI buffer truncating not implemented, clearing" << std::endl;
            clear();
        }
    }

    void for_each_msg(std::function<void(TimeType t, SizeType s, uint8_t* data)> cb) {
        m_storage->for_each_msg(cb);
    }

    struct Message {
        TimeType time;
        SizeType size;
        std::vector<uint8_t> data;
    };

    std::vector<Message> retrieve_contents(size_t &length_out, bool thread_safe = true) {
        static constexpr auto poll_interval = 10ms;

        auto s = std::make_shared<Storage>(m_storage->bytes_capacity());
        if (thread_safe) {
            std::cerr << "Warning: no timeout mechanism implemented" << std::endl;
            DataSnapshot d;
            d.data = s;
            maybe_copy_data_to = &d;
            while (maybe_copy_data_to != nullptr) {
                std::this_thread::sleep_for(poll_interval);
            }
            length_out = d.length;
        } else {
            m_storage->copy(*s);
            length_out = get_length();
        }
        std::vector<Message> r;
        s->for_each_msg([&r](TimeType time, SizeType size, uint8_t*data) {
            r.push_back({.time = time, .size = size, .data = std::vector<uint8_t>(size)});
            memcpy((void*)r.back().data.data(), (void*)data, size);
        });
        return r;
    }

    void set_contents(std::vector<Message> contents, size_t length, bool thread_safe = true) {
        static constexpr auto poll_interval = 10ms;

        auto s = std::make_shared<Storage>(m_storage->bytes_capacity());
        for(auto const& elem : contents) {
            s->append(elem.time, elem.size, elem.data.data());
        }
        if (thread_safe) {
            std::cerr << "Warning: no timeout mechanism implemented" << std::endl;
            DataSnapshot d;
            d.data = s;
            d.length = length;
            maybe_copy_data_from = &d;
            while (maybe_copy_data_from != nullptr) {
                std::this_thread::sleep_for(poll_interval);
            }
        } else {
            m_storage = s;
            set_length(length);
        }
        m_playback_cursor = m_storage->create_cursor();
    }
};