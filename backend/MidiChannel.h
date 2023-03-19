#pragma once
#include "MidiStorage.h"
#include "MidiPortInterface.h"
#include "ChannelInterface.h"
#include "WithCommandQueue.h"
#include "MidiMessage.h"
#include "MidiNotesState.h"
#include "modified_loop_mode_for_channel.h"
#include <optional>
#include <functional>
#include <chrono>
#include <thread>

using namespace std::chrono_literals;

template<typename TimeType, typename SizeType>
class MidiChannel : public ChannelInterface,
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
    std::unique_ptr<MidiNotesState> mp_output_notes_state;

    // Any thread access
    std::atomic<channel_mode_t> ma_mode;
    std::atomic<size_t> ma_data_length; // Length in samples
    std::atomic<size_t> ma_n_events_triggered;

public:
    MidiChannel(size_t data_size, channel_mode_t mode) :
        WithCommandQueue<10, 1000, 1000>(),
        mp_playback_target_buffer(nullptr),
        mp_recording_source_buffer(nullptr),
        mp_storage(std::make_shared<Storage>(data_size)),
        mp_playback_cursor(nullptr),
        ma_mode(mode),
        ma_data_length(0),
        mp_output_notes_state(std::make_unique<MidiNotesState>()),
        ma_n_events_triggered(0)
    {
        mp_playback_cursor = mp_storage->create_cursor();
    }

    // NOTE: only use on process thread
    MidiChannel<TimeType, SizeType>& operator= (MidiChannel<TimeType, SizeType> const& other) {
        mp_playback_target_buffer = other.mp_playback_target_buffer;
        mp_recording_source_buffer = other.mp_recording_source_buffer;
        mp_storage = other.mp_storage;
        mp_playback_cursor = other.mp_playback_cursor;
        ma_mode = other.ma_mode;
        ma_n_events_triggered = other.ma_n_events_triggered;
        mp_output_notes_state = other.mp_output_notes_state;
        return *this;
    }

    size_t get_length() const override { return ma_data_length; }
    void PROC_set_length(size_t length) override {
        mp_storage->truncate(length);
        ma_data_length = length;
    }

#warning Replace use-case not covered for MIDI.
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

        loop_mode_t modified_mode = modified_loop_mode_for_channel(mode_before, ma_mode);

        switch (modified_mode) {
            case Playing:
                PROC_process_playback(pos_before, length_before, n_samples, false);
                break;
            case Recording:
                PROC_process_record(length_before, n_samples);
                break;
            case Stopped:
            case Replacing:
                break;
            default:
                break;
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

        // Truncate buffer if necessary
        PROC_set_length(our_length);
        
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
            const uint8_t *d;
            recbuf.buf->PROC_get_event_reference(idx).get(s, t, d);
            if (t >= record_end) {
                break;
            }
            mp_storage->append(our_length + (TimeType) t, (SizeType) s, d);
            recbuf.n_events_processed++;
        }
        
        recbuf.n_frames_processed += n_samples;
        ma_data_length += n_samples;
    }

    void clear(bool thread_safe=true) {
        auto fn = [this]() {
            mp_storage->clear();
            mp_playback_cursor.reset();
            mp_output_notes_state->clear();
            ma_n_events_triggered = 0;
            PROC_set_length(0);
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
                if (buf.buf->write_by_reference_supported()) {
                    buf.buf->PROC_write_event_reference(*event);
                    uint32_t time,size;
                    const uint8_t *data;
                    event->get(time, size, data);
                    mp_output_notes_state->process_msg(data);
                } else if (buf.buf->write_by_value_supported()) {
                    buf.buf->PROC_write_event_value(event->size, event_time, event->data());
                    mp_output_notes_state->process_msg(event->data());
                } else {
                    throw std::runtime_error("Midi write buffer does not support any write methods");
                }
                ma_n_events_triggered++;
            }
            buf.n_events_processed++;
            mp_playback_cursor->next();
        }
        
        buf.n_frames_processed += n_samples;
    }

    std::optional<size_t> PROC_get_next_poi(loop_mode_t mode,
                                               size_t length,
                                               size_t position) const override {
        if (ma_mode) {
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
            r.push_back(Message(time, size, std::vector<uint8_t>(size)));
            memcpy((void*)r.back().data.data(), (void*)data, size);
        });
        return r;
    }

    void set_contents(std::vector<Message> contents, size_t length_samples, bool thread_safe = true) {
        auto s = std::make_shared<Storage>(mp_storage->bytes_capacity());
        for(auto const& elem : contents) {
            s->append(elem.time, elem.size, elem.data.data());
        }

        auto fn = [this, &s, length_samples]() {
            mp_storage = s;
            mp_playback_cursor = mp_storage->create_cursor();
            PROC_set_length(length_samples);
        };

        if (thread_safe) { exec_process_thread_command(fn); }
        else { fn(); }
    }

    void PROC_handle_poi(loop_mode_t mode,
                    size_t length,
                    size_t position) override {}

    void set_mode(channel_mode_t mode) override {
        ma_mode = mode;
    }

    channel_mode_t get_mode() const override {
        return ma_mode;
    }

    size_t get_n_notes_active() const {
        return mp_output_notes_state->n_notes_active();
    }

    size_t get_n_events_triggered() {
        auto rval = ma_n_events_triggered.load();
        ma_n_events_triggered = 0;
        return rval;
    }
};