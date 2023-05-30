#pragma once
#include "MidiStorage.h"
#include "MidiPortInterface.h"
#include "ChannelInterface.h"
#include "WithCommandQueue.h"
#include "MidiMessage.h"
#include "MidiStateTracker.h"
#include "MidiStateDiffTracker.h"
#include "channel_mode_helpers.h"
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
    std::shared_ptr<Storage>       mp_prerecord_storage;
    std::shared_ptr<StorageCursor> mp_playback_cursor;
    std::shared_ptr<MidiStateTracker> mp_output_midi_state;
    std::shared_ptr<MidiStateTracker> mp_midi_state_at_record_start;
    std::shared_ptr<MidiStateDiffTracker> mp_midi_state_diff_wrt_record_start;
    size_t mp_prev_pos_after;
    unsigned mp_prev_process_flags;

    // Any thread access
    std::atomic<channel_mode_t> ma_mode;
    std::atomic<size_t> ma_data_length; // Length in samples
    std::atomic<size_t> ma_prerecord_data_length;
    std::atomic<int> ma_start_offset;
    std::atomic<size_t> ma_n_events_triggered;
    std::atomic<unsigned> ma_data_seq_nr;
    std::atomic<size_t> ma_pre_play_samples;
    std::atomic<int> ma_last_played_back_sample;

    const Message all_sound_off_message_channel_0 = Message(0, 3, {0xB0, 120, 0});

public:
    MidiChannel(size_t data_size, channel_mode_t mode) :
        WithCommandQueue<10, 1000, 1000>(),
        mp_playback_target_buffer(nullptr),
        mp_recording_source_buffer(nullptr),
        mp_storage(std::make_shared<Storage>(data_size)),
        mp_prerecord_storage(std::make_shared<Storage>(data_size)),
        mp_playback_cursor(nullptr),
        ma_mode(mode),
        ma_data_length(0),
        mp_output_midi_state(std::make_shared<MidiStateTracker>(true, true, true)),
        mp_midi_state_at_record_start(std::make_shared<MidiStateTracker>()),
        mp_midi_state_diff_wrt_record_start(std::make_shared<MidiStateDiffTracker>()),
        ma_n_events_triggered(0),
        ma_start_offset(0),
        ma_data_seq_nr(0),
        ma_pre_play_samples(0),
        mp_prev_pos_after(0),
        mp_prev_process_flags(0),
        ma_last_played_back_sample(0),
        ma_prerecord_data_length(0)
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
        mp_output_midi_state = other.mp_output_midi_state;
        mp_midi_state_at_record_start = other.mp_midi_state_at_record_start;
        mp_midi_state_diff_wrt_record_start = other.mp_midi_state_diff_wrt_record_start;
        ma_start_offset = other.ma_start_offset.load();
        ma_data_length = other.ma_data_length.load();
        mp_prev_process_flags = other.mp_prev_process_flags;
        mp_prerecord_storage = other.mp_prerecord_storage;
        ma_prerecord_data_length = other.ma_prerecord_data_length;
        data_changed();
        return *this;
    }

    unsigned get_data_seq_nr() const override { return ma_data_seq_nr; }

    void data_changed() { ma_data_seq_nr++; }

    size_t get_length() const override { return ma_data_length; }
    
    void PROC_set_length_impl(Storage &storage, std::atomic<size_t> &storage_length, size_t length) {
        if (storage_length != length) {
            storage.truncate(length);
            storage_length = length;
            data_changed();
        }
    }
    
    void PROC_set_length(size_t length) override {
        PROC_set_length_impl(*mp_storage, ma_data_length, length);
    }

    void set_pre_play_samples(size_t samples) override {
        ma_pre_play_samples = samples;
    }
    
    size_t get_pre_play_samples() const override {
        return ma_pre_play_samples;
    }

    // MIDI channels process everything immediately. Deferred processing is not possible.
#warning Replace use-case not covered for MIDI.
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

        // If we were playing back and anything other than forward playback is happening
        // (e.g. mode switch, position set, ...), send an All Sound Off.
        // Also reset the playback cursor in that case.
        if ((process_flags & ChannelPlayback) && (
            (!process_flags & ChannelPlayback) ||
            pos_before != mp_prev_pos_after
        )) {
            PROC_send_all_sound_off();
            mp_playback_cursor->reset();
        }

        if (!(process_flags & ChannelPreRecord) &&
             (mp_prev_process_flags & ChannelPreRecord))
        {
            // Ending pre-record. If transitioning to recording,
            // make our pre-recorded buffers into our main buffers.
            // Otherwise, just discard them.
            if (process_flags & ChannelRecord) {
                mp_storage = mp_prerecord_storage;
                ma_data_length = ma_start_offset = ma_prerecord_data_length.load();
            }
            mp_prerecord_storage = std::make_shared<Storage>(mp_storage->bytes_capacity());
            ma_prerecord_data_length = 0;
        }

        if (process_flags & ChannelPlayback) {
            ma_last_played_back_sample = process_params.position;
            PROC_process_playback(process_params.position, length_before, n_samples, false);
        } else {
            ma_last_played_back_sample = -1;
        }
        if (process_flags & ChannelRecord) {
            PROC_process_record(*mp_storage, ma_data_length, length_before, n_samples);
        }
        if (process_flags & ChannelPreRecord) {
            PROC_process_record(*mp_prerecord_storage, ma_prerecord_data_length, ma_prerecord_data_length, n_samples);
        }

        mp_prev_pos_after = pos_after;
        mp_prev_process_flags = process_flags;


        // Update recording/playback buffers.
        if(mp_recording_source_buffer.has_value()) {
            mp_recording_source_buffer->n_frames_processed += n_samples;
        }
        if(mp_playback_target_buffer.has_value()) {
            mp_playback_target_buffer->n_frames_processed += n_samples;
        }
    }

    void PROC_finalize_process() override {}

    void PROC_process_record(Storage &storage,
                             std::atomic<size_t> &storage_data_length,
                             size_t record_from,
                             size_t n_samples) {
        if (!mp_recording_source_buffer.has_value()) {
            throw std::runtime_error("Recording without source buffer");
        }
        auto &recbuf = mp_recording_source_buffer.value();
        if (recbuf.frames_left() < n_samples) {
            throw std::runtime_error("Attempting to record out of bounds");
        }
        bool changed = false;

        // Truncate buffer if necessary
        PROC_set_length_impl(storage, storage_data_length, record_from);
        
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
            if (t >= recbuf.n_frames_processed) { // May need to skip messages
                // If here, we are about to record a message.
                // If it is the first one, this is also the moment to cache the
                // MIDI state (such as hold pedal, other CCs, pitch wheel, etc.)
                if (storage.n_events() == 0) {
                    *mp_midi_state_at_record_start = *mp_output_midi_state;
                    mp_midi_state_diff_wrt_record_start->reset(mp_midi_state_at_record_start, mp_output_midi_state, StateDiffTrackerAction::ClearDiff);
                }
 
                storage.append(record_from + (TimeType) t, (SizeType) s, d);
                changed = true;
            }
            recbuf.n_events_processed++;
        }
        
        PROC_set_length_impl(storage, storage_data_length, storage_data_length + n_samples);
        if (changed) { data_changed(); }
    }

    void clear(bool thread_safe=true) {
        auto fn = [this]() {
            mp_storage->clear();
            mp_playback_cursor->reset();
            mp_output_midi_state->clear();
            mp_midi_state_at_record_start->clear();
            ma_n_events_triggered = 0;
            PROC_set_length(0);
            ma_start_offset = 0;
            data_changed();
        };
        if (thread_safe) { exec_process_thread_command(fn); }
        else { fn(); }
    }

    void PROC_send_all_sound_off() {
        auto &buf = mp_playback_target_buffer.value();
        if (buf.frames_left() < 1) {
            throw std::runtime_error("Attempting to play back out of bounds");
        }
        PROC_send_message(*buf.buf, all_sound_off_message_channel_0);
    }

    template<typename Buf, typename Event>
    void PROC_send_message(Buf &buf, Event &event) {
        if (buf.write_by_reference_supported()) {
            buf.PROC_write_event_reference(event);
            mp_output_midi_state->process_msg(event.get_data());
        } else if (buf.write_by_value_supported()) {
            buf.PROC_write_event_value(event.size, event.get_time(), event.get_data());
            mp_output_midi_state->process_msg(event.get_data());
        } else {
            throw std::runtime_error("Midi write buffer does not support any write methods");
        }
    }

    void PROC_process_playback(size_t our_pos, size_t our_length, size_t n_samples, bool muted) {
        if (!mp_playback_target_buffer.has_value()) {
            throw std::runtime_error("Playing without target buffer");
        }
        auto &buf = mp_playback_target_buffer.value();
        if (buf.frames_left() < n_samples) {
            throw std::runtime_error("Attempting to play back out of bounds");
        }
        auto _pos = (int)our_pos;

        // Playback any events
        size_t end = buf.n_frames_processed + n_samples;
        mp_playback_cursor->find_time_forward(std::max(0, _pos));

        int valid_from = std::max(_pos, (int)ma_start_offset - (int)ma_pre_play_samples);
        int valid_to = _pos + (int)n_samples;
        while(mp_playback_cursor->valid())
        {
            auto *event = mp_playback_cursor->get();
            if ((int)event->storage_time >= valid_to) {
                // Future event
                break;
            }
            if (!muted && (int)event->storage_time >= valid_from) {
                event->proc_time = (int)event->storage_time - _pos + buf.n_frames_processed;
                PROC_send_message(*buf.buf, *event);
                ma_n_events_triggered++;
            }
            buf.n_events_processed++;
            mp_playback_cursor->next();
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
                merge_poi(mp_playback_target_buffer.value().n_frames_total - mp_playback_target_buffer.value().n_frames_processed);
            }
            if (process_params.process_flags & (ChannelRecord | ChannelPreRecord)) {
                merge_poi(mp_recording_source_buffer.value().n_frames_total - mp_recording_source_buffer.value().n_frames_processed);
            }
        }
        return rval;
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
            data_changed();
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
        return mp_output_midi_state->n_notes_active();
    }

    size_t get_n_events_triggered() {
        auto rval = ma_n_events_triggered.load();
        ma_n_events_triggered = 0;
        return rval;
    }

    void set_start_offset(int offset) override {
        ma_start_offset = offset;
    }

    int get_start_offset() const override {
        return ma_start_offset;
    }

    std::optional<size_t> get_played_back_sample() const override {
        return std::nullopt;
    }
};