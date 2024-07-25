#pragma once
#include "MidiStorage.h"
#include "ChannelInterface.h"
#include "WithCommandQueue.h"
#include "MidiMessage.h"
#include "MidiStateTracker.h"
#include "LoggingEnabled.h"
#include "ProcessProfiling.h"
#include "TrackedRelativeMidiState.h"
#include <stdint.h>
#include "shoop_shared_ptr.h"

class MidiChannel : public ChannelInterface,
                    private WithCommandQueue,
                    private ModuleLoggingEnabled<"Backend.MidiChannel"> {
public:
    using Storage = MidiStorage;
    using StorageCursor = typename Storage::Cursor;
    using Message = MidiMessage<uint32_t, uint16_t>;

private:

    struct ExternalBufState {
        uint32_t n_events_total = 0;
        uint32_t n_frames_total = 0;
        uint32_t n_events_processed = 0;
        uint32_t n_frames_processed = 0;

        ExternalBufState();
        
        uint32_t frames_left() const;
        uint32_t events_left() const;
    };

    // Process thread access only (mp*)
    std::optional<std::pair<ExternalBufState, MidiWriteableBufferInterface*>> mp_playback_target_buffer = std::nullopt;
    std::optional<std::pair<ExternalBufState, MidiReadableBufferInterface*>> mp_recording_source_buffer = std::nullopt;
    shoop_shared_ptr<Storage>       mp_storage = nullptr;
    shoop_shared_ptr<Storage>       mp_prerecord_storage = nullptr;
    shoop_shared_ptr<StorageCursor> mp_playback_cursor = nullptr;
    shoop_shared_ptr<profiling::ProfilingItem> mp_profiling_item = nullptr;

    // Holds and updates the current MIDI state on the channel output.
    shoop_shared_ptr<MidiStateTracker> mp_output_midi_state = nullptr;

    // Holds and updates the current MIDI state on the channel input.
    shoop_shared_ptr<MidiStateTracker> mp_input_midi_state = nullptr;

    // Hold the state that the MIDI channel output had at the time of the recording
    // start. Also keep track of the difference between this state and the current state.
    shoop_shared_ptr<TrackedRelativeMidiState> mp_recording_start_state_tracker;

    // While pre-recording, we save our start state tracker separately because it might
    // need to be discarded and the old one kept. Once pre-recording turns into actual
    // recording, the start state tracker will be moved to mp_recording_start_state_tracker.
    shoop_shared_ptr<TrackedRelativeMidiState> mp_temp_prerecording_start_state_tracker;

    // When (pre-)playback starts, the channel may be muted or not at its start offset
    // sample yet. Messages may be skipped and not played because of this. Once an actual
    // note needs to be played, we still need to send any additional messages necessary
    // to first achieve the state that was supposed to be at this point in playback.
    // This tracker is used to track unplayed MIDI messages as if they were played, so
    // that the correct state can be reached.
    shoop_shared_ptr<TrackedRelativeMidiState> mp_track_state_until_first_msg_playback;

    uint32_t mp_prev_pos_after = 0;
    unsigned mp_prev_process_flags = 0;

    // Any thread access
    std::atomic<shoop_channel_mode_t> ma_mode = ChannelMode_Direct;
    std::atomic<uint32_t> ma_data_length = 0; // Length in samples
    std::atomic<uint32_t> ma_prerecord_data_length = 0;
    std::atomic<int> ma_start_offset = 0;
    std::atomic<uint32_t> ma_n_events_triggered = 0;
    std::atomic<unsigned> ma_data_seq_nr = 0;
    std::atomic<uint32_t> ma_pre_play_samples = 0;
    std::atomic<int> ma_last_played_back_sample = 0;

public:
    MidiChannel(uint32_t data_size, shoop_channel_mode_t mode);
    ~MidiChannel();

    // NOTE: only use on process thread
    MidiChannel& operator= (MidiChannel const& other);

    unsigned get_data_seq_nr() const override;

    void data_changed();

    uint32_t get_length() const override;
    
    void PROC_set_length_impl(Storage &storage, std::atomic<uint32_t> &storage_length, uint32_t length);
    
    void PROC_set_length(uint32_t length) override;

    void set_pre_play_samples(uint32_t samples) override;
    
    uint32_t get_pre_play_samples() const override;

    // MIDI channels process everything immediately. Deferred processing is not possible.
    void PROC_process(
        shoop_loop_mode_t mode,
        std::optional<shoop_loop_mode_t> maybe_next_mode,
        std::optional<uint32_t> maybe_next_mode_delay_cycles,
        std::optional<uint32_t> maybe_next_mode_eta,
        uint32_t n_samples,
        uint32_t pos_before,
        uint32_t pos_after,
        uint32_t length_before,
        uint32_t length_after
        ) override;

    void PROC_finalize_process() override;
    void PROC_process_input_messages(uint32_t n_samples);
    void PROC_process_record(Storage &storage,
                             std::atomic<uint32_t> &storage_data_length,
                             TrackedRelativeMidiState &track_start_state,
                             uint32_t record_from,
                             uint32_t n_samples);

    void clear(bool thread_safe=true);
    void PROC_send_all_sound_off(unsigned frame=0);

    void PROC_send_message_ref(MidiWriteableBufferInterface &buf, MidiSortableMessageInterface const &event);

    void PROC_send_message_value(MidiWriteableBufferInterface &buf, uint32_t time, uint32_t size, uint8_t *data);
    void PROC_process_playback(uint32_t our_pos, uint32_t our_length, uint32_t n_samples, bool muted);

    std::optional<uint32_t> PROC_get_next_poi(shoop_loop_mode_t mode,
                                               std::optional<shoop_loop_mode_t> maybe_next_mode,
                                               std::optional<uint32_t> maybe_next_mode_delay_cycles,
                                               std::optional<uint32_t> maybe_next_mode_eta,
                                               uint32_t length,
                                               uint32_t position) const override;

    void PROC_set_playback_buffer(MidiWriteableBufferInterface *buffer, uint32_t n_frames);

    void PROC_set_recording_buffer(MidiReadableBufferInterface *buffer, uint32_t n_frames);

    struct Contents {
        std::vector<Message> recorded_msgs;
        std::vector<std::vector<uint8_t>> starting_state_msg_datas;
    };

    Contents retrieve_contents(bool thread_safe = true);

    void set_contents(
        Contents contents,
        uint32_t length_samples,
        bool thread_safe = true);

    void PROC_handle_poi(shoop_loop_mode_t mode,
                    uint32_t length,
                    uint32_t position) override;

    void set_mode(shoop_channel_mode_t mode) override;

    shoop_channel_mode_t get_mode() const override;

    uint32_t get_n_notes_active() const;

    uint32_t get_n_events_triggered();

    void set_start_offset(int offset) override;

    int get_start_offset() const override;

    std::optional<uint32_t> get_played_back_sample() const override;
    
    void adopt_ringbuffer_contents(shoop_shared_ptr<PortInterface> from_port,
        std::optional<unsigned> keep_samples_before_start_offset,
        std::optional<unsigned> reverse_start_offset,
        bool thread_safe=true) override;
    
    // Forget about any tracked MIDI state
    void PROC_reset_midi_state_tracking();
};