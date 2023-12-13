#include "AudioMidiLoop.h"
#include "WithCommandQueue.h"
#include "ObjectPool.h"
#include <memory>
#include <vector>

AudioMidiLoop::AudioMidiLoop()
    : BasicLoop() {}

template <typename SampleT>
std::shared_ptr<AudioChannel<SampleT>> AudioMidiLoop::add_audio_channel(
    std::shared_ptr<ObjectPool<AudioBuffer<SampleT>>> const &buffer_pool,
    uint32_t initial_max_buffers, shoop_channel_mode_t mode, bool thread_safe)
{
    log_trace();

    auto channel = std::make_shared<AudioChannel<SampleT>>(
        buffer_pool, initial_max_buffers, mode);
    auto fn = [this, channel]() { mp_audio_channels.push_back(channel); };
    if (thread_safe) {
        exec_process_thread_command(fn);
    } else {
        fn();
    }
    return channel;
}

template <typename TimeType, typename SizeType>
std::shared_ptr<MidiChannel<TimeType, SizeType>>
AudioMidiLoop::add_midi_channel(uint32_t data_size, shoop_channel_mode_t mode,
                                bool thread_safe)
{
    log_trace();

    auto channel = std::make_shared<MidiChannel<TimeType, SizeType>>(
        data_size, mode);
    auto fn = [this, &channel]() { mp_midi_channels.push_back(channel); };
    if (thread_safe) {
        exec_process_thread_command(fn);
    } else {
        fn();
    }

    return channel;
}

template <typename TimeType, typename SizeType>
std::shared_ptr<MidiChannel<TimeType, SizeType>>
AudioMidiLoop::midi_channel(uint32_t idx, bool thread_safe) {
    std::shared_ptr<ChannelInterface> iface;
    if (thread_safe) {
        exec_process_thread_command(
            [this, idx, &iface]() { iface = mp_midi_channels.at(idx); });
    } else {
        iface = mp_midi_channels.at(idx);
    }

    auto maybe_r =
        std::dynamic_pointer_cast<MidiChannel<TimeType, SizeType>>(iface);
    if (!maybe_r) {
        throw std::runtime_error("Midi channel " + std::to_string(idx) +
                                 " is not of the requested channel type.");
    }
    return maybe_r;
}

template <typename SampleT>
std::shared_ptr<AudioChannel<SampleT>>
AudioMidiLoop::audio_channel(uint32_t idx, bool thread_safe) {
    std::shared_ptr<ChannelInterface> iface;
    if (thread_safe) {
        exec_process_thread_command(
            [this, idx, &iface]() { iface = mp_audio_channels.at(idx); });
    } else {
        iface = mp_audio_channels.at(idx);
    }

    auto maybe_r = std::dynamic_pointer_cast<AudioChannel<SampleT>>(iface);
    if (!maybe_r) {
        throw std::runtime_error("Audio channel " + std::to_string(idx) +
                                 " is not of the requested channel type.");
    }
    return maybe_r;
}

uint32_t AudioMidiLoop::n_audio_channels(bool thread_safe) {
    uint32_t rval;
    if (thread_safe) {
        exec_process_thread_command(
            [this, &rval]() { rval = mp_audio_channels.size(); });
    } else {
        rval = mp_audio_channels.size();
    }
    return rval;
}

uint32_t AudioMidiLoop::n_midi_channels(bool thread_safe) {
    uint32_t rval;
    if (thread_safe) {
        exec_process_thread_command(
            [this, &rval]() { rval = mp_midi_channels.size(); });
    } else {
        rval = mp_midi_channels.size();
    }
    return rval;
}

void AudioMidiLoop::delete_audio_channel(std::shared_ptr<ChannelInterface> chan,
                                         bool thread_safe) {
    log_trace();
    auto fn = [=, this]() {
        std::erase_if(mp_audio_channels,
                      [&](auto const &e) { return e.get() == chan.get(); });
    };
    if (thread_safe) {
        exec_process_thread_command(fn);
    } else {
        fn();
    }
}

void AudioMidiLoop::delete_midi_channel(std::shared_ptr<ChannelInterface> chan,
                                        bool thread_safe) {
    log_trace();
    auto fn = [=, this]() {
        std::erase_if(mp_midi_channels,
                      [&](auto const &e) { return e.get() == chan.get(); });
    };
    if (thread_safe) {
        exec_process_thread_command(fn);
    } else {
        fn();
    }
}

void AudioMidiLoop::PROC_process_channels(
    shoop_loop_mode_t mode, std::optional<shoop_loop_mode_t> maybe_next_mode,
    std::optional<uint32_t> maybe_next_mode_delay_cycles,
    std::optional<uint32_t> maybe_next_mode_eta, uint32_t n_samples,
    uint32_t pos_before, uint32_t pos_after, uint32_t length_before,
    uint32_t length_after) {
    log_trace();
    BasicLoop::PROC_process_channels(mode, maybe_next_mode,
                                     maybe_next_mode_delay_cycles,
                                     maybe_next_mode_eta, n_samples, pos_before,
                                     pos_after, length_before, length_after);

    for (auto &aloop : mp_audio_channels) {
        aloop->PROC_process(mode, maybe_next_mode, maybe_next_mode_delay_cycles,
                            maybe_next_mode_eta, n_samples, pos_before,
                            pos_after, length_before, length_after);
    }
    for (auto &mloop : mp_midi_channels) {
        mloop->PROC_process(mode, maybe_next_mode, maybe_next_mode_delay_cycles,
                            maybe_next_mode_eta, n_samples, pos_before,
                            pos_after, length_before, length_after);
    }
}

void AudioMidiLoop::PROC_update_poi() {
    log_trace();
    BasicLoop::PROC_update_poi(); // sets mp_next_poi, mp_next_trigger

    auto merge = [this](std::optional<uint32_t> other) {
        if (other.has_value() && (!mp_next_poi.has_value() ||
                                  other.value() < mp_next_poi.value().when)) {
            mp_next_poi = {other.value(), ChannelPOI};
        }
    };

    for (auto &channel : mp_audio_channels) {
        merge(channel->PROC_get_next_poi(
            get_mode(), ma_maybe_next_planned_mode, ma_maybe_next_planned_delay,
            mp_next_trigger, get_length(), get_position()));
    }
    for (auto &channel : mp_midi_channels) {
        merge(channel->PROC_get_next_poi(
            get_mode(), ma_maybe_next_planned_mode, ma_maybe_next_planned_delay,
            mp_next_trigger, get_length(), get_position()));
    }
}

void AudioMidiLoop::PROC_handle_poi() {
    log_trace();
    BasicLoop::PROC_handle_poi();

    for (auto &channel : mp_audio_channels) {
        channel->PROC_handle_poi(get_mode(), get_length(), get_position());
    }
    for (auto &channel : mp_midi_channels) {
        channel->PROC_handle_poi(get_mode(), get_length(), get_position());
    }
}

template std::shared_ptr<AudioChannel<float>> AudioMidiLoop::add_audio_channel(
    std::shared_ptr<ObjectPool<AudioBuffer<float>>> const &buffer_pool,
    uint32_t initial_max_buffers, shoop_channel_mode_t mode, bool thread_safe);

template std::shared_ptr<AudioChannel<int>> AudioMidiLoop::add_audio_channel(
    std::shared_ptr<ObjectPool<AudioBuffer<int>>> const &buffer_pool,
    uint32_t initial_max_buffers, shoop_channel_mode_t mode, bool thread_safe);

template std::shared_ptr<AudioChannel<float>>
AudioMidiLoop::audio_channel(uint32_t idx, bool thread_safe);
template std::shared_ptr<AudioChannel<int>>
AudioMidiLoop::audio_channel(uint32_t idx, bool thread_safe);

template std::shared_ptr<MidiChannel<uint32_t, uint16_t>>
AudioMidiLoop::midi_channel<uint32_t, uint16_t>(uint32_t idx, bool thread_safe);
template std::shared_ptr<MidiChannel<uint32_t, uint32_t>>
AudioMidiLoop::midi_channel<uint32_t, uint32_t>(uint32_t idx, bool thread_safe);
template std::shared_ptr<MidiChannel<uint16_t, uint16_t>>
AudioMidiLoop::midi_channel<uint16_t, uint16_t>(uint32_t idx, bool thread_safe);
template std::shared_ptr<MidiChannel<uint16_t, uint32_t>>
AudioMidiLoop::midi_channel<uint16_t, uint32_t>(uint32_t idx, bool thread_safe);

template std::shared_ptr<MidiChannel<uint32_t, uint16_t>>
AudioMidiLoop::add_midi_channel<uint32_t, uint16_t>(uint32_t data_size,
                                                    shoop_channel_mode_t mode,
                                                    bool thread_safe);
template std::shared_ptr<MidiChannel<uint32_t, uint32_t>>
AudioMidiLoop::add_midi_channel<uint32_t, uint32_t>(uint32_t data_size,
                                                    shoop_channel_mode_t mode,
                                                    bool thread_safe);
template std::shared_ptr<MidiChannel<uint16_t, uint16_t>>
AudioMidiLoop::add_midi_channel<uint16_t, uint16_t>(uint32_t data_size,
                                                    shoop_channel_mode_t mode,
                                                    bool thread_safe);
template std::shared_ptr<MidiChannel<uint16_t, uint32_t>>
AudioMidiLoop::add_midi_channel<uint16_t, uint32_t>(uint32_t data_size,
                                                    shoop_channel_mode_t mode,
                                                    bool thread_safe);
