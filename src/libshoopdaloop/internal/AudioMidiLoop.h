#include "AudioChannel.h"
#include "BasicLoop.h"
#include "ChannelInterface.h"
#include "LoggingEnabled.h"
#include "MidiChannel.h"
#include "ProcessProfiling.h"
#include <optional>

// Extend a BasicLoop with a set of audio and midi channels.
class AudioMidiLoop : public BasicLoop {
    std::vector<std::shared_ptr<ChannelInterface>> mp_audio_channels;
    std::vector<std::shared_ptr<ChannelInterface>> mp_midi_channels;

  public:
    AudioMidiLoop();

    template <typename SampleT>
    std::shared_ptr<AudioChannel<SampleT>>
    add_audio_channel(std::shared_ptr<ObjectPool<AudioBuffer<SampleT>>> const &buffer_pool,
                      uint32_t initial_max_buffers, shoop_channel_mode_t mode,
                      bool thread_safe = true);

    template <typename SampleT>
    std::shared_ptr<AudioChannel<SampleT>>
    audio_channel(uint32_t idx, bool thread_safe = true);

    template <typename TimeType, typename SizeType>
    std::shared_ptr<MidiChannel<TimeType, SizeType>>
    add_midi_channel(uint32_t data_size, shoop_channel_mode_t mode,
                     bool thread_safe = true);

    template <typename TimeType, typename SizeType>
    std::shared_ptr<MidiChannel<TimeType, SizeType>>
    midi_channel(uint32_t idx, bool thread_safe = true);

    uint32_t n_audio_channels(bool thread_safe = true);

    uint32_t n_midi_channels(bool thread_safe = true);

    void delete_audio_channel(std::shared_ptr<ChannelInterface> chan,
                              bool thread_safe = true);

    void delete_midi_channel(std::shared_ptr<ChannelInterface> chan,
                             bool thread_safe = true);

    void
    PROC_process_channels(shoop_loop_mode_t mode,
                          std::optional<shoop_loop_mode_t> maybe_next_mode,
                          std::optional<uint32_t> maybe_next_mode_delay_cycles,
                          std::optional<uint32_t> maybe_next_mode_eta,
                          uint32_t n_samples, uint32_t pos_before, uint32_t pos_after,
                          uint32_t length_before, uint32_t length_after) override;

    void PROC_update_poi() override;

    void PROC_handle_poi() override;

    // Channels may support constanly running ringbuffers. This function
    // tells them to take their ringbuffer contents and set them as the
    // channel data.
    // @param reverse_start_offset_cycle: Where to set the playback start point in terms
    //  of sync source cycles. 0 means to use the data since the current cycle
    //  started, 1 means the start of the last completed cycle, etc.
    //  nullopt will just use start offset 0.
    void adopt_ringbuffer_contents(
        std::optional<uint32_t> reverse_start_offset_cycle = std::nullopt,
        std::optional<uint32_t> n_cycles_length = std::nullopt,
        bool thread_safe=true);
};

extern template std::shared_ptr<AudioChannel<float>> AudioMidiLoop::add_audio_channel(
    std::shared_ptr<ObjectPool<AudioBuffer<float>>> const &buffer_pool,
    uint32_t initial_max_buffers, shoop_channel_mode_t mode,
    bool thread_safe
);
extern template std::shared_ptr<AudioChannel<int>> AudioMidiLoop::add_audio_channel(
    std::shared_ptr<ObjectPool<AudioBuffer<int>>> const &buffer_pool,
    uint32_t initial_max_buffers, shoop_channel_mode_t mode,
    bool thread_safe
);
extern template std::shared_ptr<AudioChannel<float>> AudioMidiLoop::audio_channel(uint32_t idx, bool thread_safe);
extern template std::shared_ptr<AudioChannel<int>> AudioMidiLoop::audio_channel(uint32_t idx, bool thread_safe);
extern template std::shared_ptr<MidiChannel<uint32_t, uint16_t>> AudioMidiLoop::add_midi_channel<uint32_t, uint16_t>(uint32_t data_size, shoop_channel_mode_t mode, bool thread_safe);
extern template std::shared_ptr<MidiChannel<uint32_t, uint32_t>> AudioMidiLoop::add_midi_channel<uint32_t, uint32_t>(uint32_t data_size, shoop_channel_mode_t mode, bool thread_safe);
extern template std::shared_ptr<MidiChannel<uint16_t, uint16_t>> AudioMidiLoop::add_midi_channel<uint16_t, uint16_t>(uint32_t data_size, shoop_channel_mode_t mode, bool thread_safe);
extern template std::shared_ptr<MidiChannel<uint16_t, uint32_t>> AudioMidiLoop::add_midi_channel<uint16_t, uint32_t>(uint32_t data_size, shoop_channel_mode_t mode, bool thread_safe);
extern template std::shared_ptr<MidiChannel<uint32_t, uint16_t>> AudioMidiLoop::midi_channel<uint32_t, uint16_t>(uint32_t idx, bool thread_safe);
extern template std::shared_ptr<MidiChannel<uint32_t, uint32_t>> AudioMidiLoop::midi_channel<uint32_t, uint32_t>(uint32_t idx, bool thread_safe);
extern template std::shared_ptr<MidiChannel<uint16_t, uint16_t>> AudioMidiLoop::midi_channel<uint16_t, uint16_t>(uint32_t idx, bool thread_safe);
extern template std::shared_ptr<MidiChannel<uint16_t, uint32_t>> AudioMidiLoop::midi_channel<uint16_t, uint32_t>(uint32_t idx, bool thread_safe);