#include "AudioChannel.h"
#include "BasicLoop.h"
#include "ChannelInterface.h"
#include "LoggingEnabled.h"
#include "MidiChannel.h"
#include "ProcessProfiling.h"

// Extend a BasicLoop with a set of audio and midi channels.
class AudioMidiLoop : public BasicLoop {
    std::vector<std::shared_ptr<ChannelInterface>> mp_audio_channels;
    std::vector<std::shared_ptr<ChannelInterface>> mp_midi_channels;
    std::shared_ptr<profiling::Profiler> maybe_profiler;

  public:
    AudioMidiLoop(
        std::shared_ptr<profiling::Profiler> maybe_profiler = nullptr);

    template <typename SampleT>
    std::shared_ptr<AudioChannel<SampleT>>
    add_audio_channel(std::shared_ptr<ObjectPool<AudioBuffer<SampleT>>> const &buffer_pool,
                      size_t initial_max_buffers, channel_mode_t mode,
                      bool thread_safe = true);

    template <typename SampleT>
    std::shared_ptr<AudioChannel<SampleT>>
    audio_channel(size_t idx, bool thread_safe = true);

    template <typename TimeType, typename SizeType>
    std::shared_ptr<MidiChannel<TimeType, SizeType>>
    add_midi_channel(size_t data_size, channel_mode_t mode,
                     bool thread_safe = true);

    template <typename TimeType, typename SizeType>
    std::shared_ptr<MidiChannel<TimeType, SizeType>>
    midi_channel(size_t idx, bool thread_safe = true);

    size_t n_audio_channels(bool thread_safe = true);

    size_t n_midi_channels(bool thread_safe = true);

    void delete_audio_channel(std::shared_ptr<ChannelInterface> chan,
                              bool thread_safe = true);

    void delete_midi_channel(std::shared_ptr<ChannelInterface> chan,
                             bool thread_safe = true);

    void
    PROC_process_channels(loop_mode_t mode,
                          std::optional<loop_mode_t> maybe_next_mode,
                          std::optional<size_t> maybe_next_mode_delay_cycles,
                          std::optional<size_t> maybe_next_mode_eta,
                          size_t n_samples, size_t pos_before, size_t pos_after,
                          size_t length_before, size_t length_after) override;

    void PROC_update_poi() override;

    void PROC_handle_poi() override;
};

extern template std::shared_ptr<AudioChannel<float>> AudioMidiLoop::add_audio_channel(
    std::shared_ptr<ObjectPool<AudioBuffer<float>>> const &buffer_pool,
    size_t initial_max_buffers, channel_mode_t mode,
    bool thread_safe
);
extern template std::shared_ptr<AudioChannel<int>> AudioMidiLoop::add_audio_channel(
    std::shared_ptr<ObjectPool<AudioBuffer<int>>> const &buffer_pool,
    size_t initial_max_buffers, channel_mode_t mode,
    bool thread_safe
);
extern template std::shared_ptr<AudioChannel<float>> AudioMidiLoop::audio_channel(size_t idx, bool thread_safe);
extern template std::shared_ptr<AudioChannel<int>> AudioMidiLoop::audio_channel(size_t idx, bool thread_safe);
extern template std::shared_ptr<MidiChannel<uint32_t, uint16_t>> AudioMidiLoop::add_midi_channel<uint32_t, uint16_t>(size_t data_size, channel_mode_t mode, bool thread_safe);
extern template std::shared_ptr<MidiChannel<uint32_t, uint32_t>> AudioMidiLoop::add_midi_channel<uint32_t, uint32_t>(size_t data_size, channel_mode_t mode, bool thread_safe);
extern template std::shared_ptr<MidiChannel<uint16_t, uint16_t>> AudioMidiLoop::add_midi_channel<uint16_t, uint16_t>(size_t data_size, channel_mode_t mode, bool thread_safe);
extern template std::shared_ptr<MidiChannel<uint16_t, uint32_t>> AudioMidiLoop::add_midi_channel<uint16_t, uint32_t>(size_t data_size, channel_mode_t mode, bool thread_safe);
extern template std::shared_ptr<MidiChannel<uint32_t, uint16_t>> AudioMidiLoop::midi_channel<uint32_t, uint16_t>(size_t idx, bool thread_safe);
extern template std::shared_ptr<MidiChannel<uint32_t, uint32_t>> AudioMidiLoop::midi_channel<uint32_t, uint32_t>(size_t idx, bool thread_safe);
extern template std::shared_ptr<MidiChannel<uint16_t, uint16_t>> AudioMidiLoop::midi_channel<uint16_t, uint16_t>(size_t idx, bool thread_safe);
extern template std::shared_ptr<MidiChannel<uint16_t, uint32_t>> AudioMidiLoop::midi_channel<uint16_t, uint32_t>(size_t idx, bool thread_safe);