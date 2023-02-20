#pragma once
#include "MidiStorage.h"
#include "MidiPortInterface.h"
#include "SubloopInterface.h"
#include "WithCommandQueue.h"
#include "MidiMessage.h"
#include <optional>
#include <functional>
#include <chrono>
#include <thread>

using namespace std::chrono_literals;

template<typename TimeType, typename SizeType>
class MidiChannelSubloop : public SubloopInterface,
                           private WithCommandQueue<10, 1000, 1000> {
public:
    using Storage = MidiStorage<TimeType, SizeType>;
    using StorageCursor = typename Storage::Cursor;
    using Message = MidiMessage<TimeType, SizeType>;

private:
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

    // Process thread access only (mp*)
    std::optional<ExternalBuf<MidiWriteableBufferInterface*>> mp_playback_target_buffer;
    std::optional<ExternalBuf<MidiReadableBufferInterface *>> mp_recording_source_buffer;
    std::shared_ptr<Storage>       mp_storage;
    std::shared_ptr<StorageCursor> mp_playback_cursor;

    // Any thread access
    std::atomic<bool> ma_enabled;

public:
    MidiChannelSubloop(size_t data_size, bool enabled) :
        WithCommandQueue<10, 1000, 1000>(),
        mp_playback_target_buffer(nullptr),
        mp_recording_source_buffer(nullptr),
        mp_storage(std::make_shared<Storage>(data_size)),
        mp_playback_cursor(nullptr),
        ma_enabled(enabled)
    {
        mp_playback_cursor = mp_storage->create_cursor();
    }

    // NOTE: only use on process thread
    MidiChannelSubloop<TimeType, SizeType>& operator= (MidiChannelSubloop<TimeType, SizeType> const& other) {
        mp_playback_target_buffer = other.mp_playback_target_buffer;
        mp_recording_source_buffer = other.mp_recording_source_buffer;
        mp_storage = other.mp_storage;
        mp_playback_cursor = other.mp_playback_cursor;
        ma_enabled = other.ma_enabled;
        return *this;
    }

    void PROC_process(
        loop_mode_t mode_before,
        loop_mode_t mode_after,
        size_t n_samples,
        size_t pos_before,
        size_t pos_after,
        size_t length_before,
        size_t length_after
        ) override {
        PROC_handle_command_queue();

        if (ma_enabled) {
            switch (mode_before) {
                case Playing:
                    PROC_process_playback(pos_before, length_before, n_samples, false);
                    break;
                case Recording:
                    PROC_process_record(length_before, n_samples);
                    break;
                default:
                    break;
            }
        }
    }

    void PROC_process_record(size_t our_length, size_t n_samples) {
        if (!mp_recording_source_buffer.has_value()) {
            throw std::runtime_error("Recording without source buffer");
        }
        auto &recbuf = mp_recording_source_buffer.value();
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
            recbuf.buf->PROC_get_event(idx, s, t, d);
            if (t >= record_end) {
                break;
            }
            mp_storage->append(our_length + (TimeType) t, (SizeType) s, d);
            recbuf.n_events_processed++;
        }
        
        recbuf.n_frames_processed += n_samples;
    }

    void clear(bool thread_safe=true) {
        auto fn = [this]() {
            mp_storage->clear();
            mp_playback_cursor.reset();
        };
        if (thread_safe) { exec_process_thread_command(fn); }
        else { fn(); }
    }

    void PROC_process_playback(size_t our_pos, size_t our_length, size_t n_samples, bool muted) {
        if (!mp_playback_target_buffer.has_value()) {
            throw std::runtime_error("Playing without target buffer");
        }
        auto &buf = mp_playback_target_buffer.value();
        if (buf.frames_left() < n_samples) {
            throw std::runtime_error("Attempting to play back out of bounds");
        }

        // Playback any events
        size_t end = buf.n_frames_processed + n_samples;
        mp_playback_cursor->find_time_forward(our_pos);
        while(mp_playback_cursor->valid())
        {
            auto *event = mp_playback_cursor->get();
            size_t event_time = event->time - our_pos;
            if (event_time >= n_samples) {
                // Future event
                break;
            }
            if (!muted) {
                buf.buf->PROC_write_event(event->size, event_time, event->data);
            }
            buf.n_events_processed++;
            mp_playback_cursor->next();
        }
        
        buf.n_frames_processed += n_samples;
    }

    std::optional<size_t> PROC_get_next_poi(loop_mode_t mode,
                                               size_t length,
                                               size_t position) const override {
        if (ma_enabled) {
            if (mode == Playing) {
                return mp_playback_target_buffer.value().n_frames_total - mp_playback_target_buffer.value().n_frames_processed;
            } else if (mode == Recording) {
                return mp_recording_source_buffer.value().n_frames_total - mp_recording_source_buffer.value().n_frames_processed;
            }
        }

        return std::nullopt;
    }

    void PROC_set_playback_buffer(MidiWriteableBufferInterface *buffer, size_t n_frames) {
        mp_playback_target_buffer = ExternalBuf<MidiWriteableBufferInterface *> (buffer);
        mp_playback_target_buffer.value().n_frames_total = n_frames;
    }

    void PROC_set_recording_buffer(MidiReadableBufferInterface *buffer, size_t n_frames) {
        mp_recording_source_buffer = ExternalBuf<MidiReadableBufferInterface *> (buffer);
        mp_recording_source_buffer.value().n_frames_total = n_frames;
        mp_recording_source_buffer.value().n_events_total = buffer->PROC_get_n_events();
    }

    std::vector<Message> retrieve_contents(bool thread_safe = true) {
        auto s = std::make_shared<Storage>(mp_storage->bytes_capacity());
        auto fn = [this, &s]() {
            mp_storage->copy(*s);
        };
        if (thread_safe) { exec_process_thread_command(fn); }
        else { fn(); }

        std::vector<Message> r;
        s->for_each_msg([&r](TimeType time, SizeType size, uint8_t*data) {
            r.push_back({.time = time, .size = size, .data = std::vector<uint8_t>(size)});
            memcpy((void*)r.back().data.data(), (void*)data, size);
        });
        return r;
    }

    void set_contents(std::vector<Message> contents, bool thread_safe = true) {
        auto s = std::make_shared<Storage>(mp_storage->bytes_capacity());
        for(auto const& elem : contents) {
            s->append(elem.time, elem.size, elem.data.data());
        }

        auto fn = [this, &s]() {
            mp_storage = s;
            mp_playback_cursor = mp_storage->create_cursor();
        };

        if (thread_safe) { exec_process_thread_command(fn); }
        else { fn(); }
    }

    void PROC_handle_poi(loop_mode_t mode,
                    size_t length,
                    size_t position) override {}
    
    void PROC_clear() {
        mp_storage->clear();
        mp_playback_cursor->reset();
    }

    void set_enabled(bool enabled) override {
        ma_enabled = enabled;
    }

    bool get_enabled() const override {
        return ma_enabled;
    }
};