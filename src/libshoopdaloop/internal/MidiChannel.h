#pragma once
#include "MidiStorage.h"
#include "ChannelInterface.h"
#include "WithCommandQueue.h"
#include "MidiMessage.h"
#include "MidiStateTracker.h"
#include "MidiStateDiffTracker.h"
#include "LoggingEnabled.h"
#include "ProcessProfiling.h"
#include <stdint.h>

template<typename TimeType, typename SizeType>
class MidiChannel : public ChannelInterface,
                    private WithCommandQueue<20, 1000, 1000>,
                    private ModuleLoggingEnabled {
public:
    using Storage = MidiStorage<TimeType, SizeType>;
    using StorageCursor = typename Storage::Cursor;
    using Message = MidiMessage<TimeType, SizeType>;

private:
    std::string log_module_name() const override;

    struct ExternalBufState {
        size_t n_events_total;
        size_t n_frames_total;
        size_t n_events_processed;
        size_t n_frames_processed;

        ExternalBufState();
        
        size_t frames_left() const;
        size_t events_left() const;
    };

    // Tracks or remembers a MIDI state (controls, pedals, etc).
    // Doesn't track notes.
    // Also keeps track of the diff between that state and the tracked state on the
    // MIDI out of the channel.
    struct TrackedState {
        bool m_valid = false;
        std::shared_ptr<MidiStateTracker> state;
        std::shared_ptr<MidiStateDiffTracker> diff;

        TrackedState(bool notes=false, bool controls=false, bool programs=false);
        TrackedState& operator= (TrackedState const& other);

        void set_from(std::shared_ptr<MidiStateTracker> &t);
        void reset();
        bool valid() const;
        void set_valid(bool v);
        void resolve_to_output(std::function<void(size_t size, uint8_t *data)> send_cb);
    };

    // Process thread access only (mp*)
    std::optional<std::pair<ExternalBufState, MidiWriteableBufferInterface*>> mp_playback_target_buffer;
    std::optional<std::pair<ExternalBufState, MidiReadableBufferInterface*>> mp_recording_source_buffer;
    std::shared_ptr<Storage>       mp_storage;
    std::shared_ptr<Storage>       mp_prerecord_storage;
    std::shared_ptr<StorageCursor> mp_playback_cursor;
    std::shared_ptr<profiling::ProfilingItem> mp_profiling_item;

    // Track the MIDI state on the channel's output.
    std::shared_ptr<MidiStateTracker> mp_output_midi_state;

    // Track the MIDI state on the channel's input.
    std::shared_ptr<MidiStateTracker> mp_input_midi_state;

    // We need to track what the MIDI state was at multiple interesting points
    // in order to restore that state quickly.
    TrackedState mp_track_start_state;
    TrackedState mp_track_prerecord_start_state;
    // We also have a state which represents what the current playback
    // state is **supposed** to be. For example, this tracks control messages
    // while the channel is muted or playing back before the first sample,
    // keeping track of the state.
    TrackedState mp_pre_playback_state;

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
    MidiChannel(size_t data_size, channel_mode_t mode, std::shared_ptr<profiling::Profiler> maybe_profiler=nullptr);
    ~MidiChannel();

    // NOTE: only use on process thread
    MidiChannel<TimeType, SizeType>& operator= (MidiChannel<TimeType, SizeType> const& other);

    unsigned get_data_seq_nr() const override;

    void data_changed();

    size_t get_length() const override;
    
    void PROC_set_length_impl(Storage &storage, std::atomic<size_t> &storage_length, size_t length);
    
    void PROC_set_length(size_t length) override;

    void set_pre_play_samples(size_t samples) override;
    
    size_t get_pre_play_samples() const override;

    // MIDI channels process everything immediately. Deferred processing is not possible.
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
        ) override;

    void PROC_finalize_process() override;
    void PROC_process_input_messages(size_t n_samples);
    void PROC_process_record(Storage &storage,
                             std::atomic<size_t> &storage_data_length,
                             TrackedState &track_start_state,
                             size_t record_from,
                             size_t n_samples);

    void clear(bool thread_safe=true);
    void PROC_send_all_sound_off();

    void PROC_send_message_ref(MidiWriteableBufferInterface &buf, MidiSortableMessageInterface const &event);

    void PROC_send_message_value(MidiWriteableBufferInterface &buf, size_t time, size_t size, uint8_t *data);
    void PROC_process_playback(size_t our_pos, size_t our_length, size_t n_samples, bool muted);

    std::optional<size_t> PROC_get_next_poi(loop_mode_t mode,
                                               std::optional<loop_mode_t> maybe_next_mode,
                                               std::optional<size_t> maybe_next_mode_delay_cycles,
                                               std::optional<size_t> maybe_next_mode_eta,
                                               size_t length,
                                               size_t position) const override;

    void PROC_set_playback_buffer(MidiWriteableBufferInterface *buffer, size_t n_frames);

    void PROC_set_recording_buffer(MidiReadableBufferInterface *buffer, size_t n_frames);

    std::vector<Message> retrieve_contents(bool thread_safe = true);

    void set_contents(std::vector<Message> contents, size_t length_samples, bool thread_safe = true);

    void PROC_handle_poi(loop_mode_t mode,
                    size_t length,
                    size_t position) override;

    void set_mode(channel_mode_t mode) override;

    channel_mode_t get_mode() const override;

    size_t get_n_notes_active() const;

    size_t get_n_events_triggered();

    void set_start_offset(int offset) override;

    int get_start_offset() const override;

    std::optional<size_t> get_played_back_sample() const override;
};

#ifndef IMPLEMENT_MIDICHANNEL_H
extern template class MidiChannel<uint32_t, uint16_t>;
extern template class MidiChannel<uint32_t, uint32_t>;
extern template class MidiChannel<uint16_t, uint16_t>;
extern template class MidiChannel<uint16_t, uint32_t>;
#endif