#include "BasicLoop.h"
#include "ChannelInterface.h"
#include "AudioChannel.h"
#include "MidiChannel.h"
#include "WithCommandQueue.h"

#include <vector>
#include <memory>

// Extend a BasicLoop with a set of audio and midi channels.
class AudioMidiLoop : public BasicLoop {
    std::vector<std::shared_ptr<ChannelInterface>> mp_audio_channels;
    std::vector<std::shared_ptr<ChannelInterface>> mp_midi_channels;
public:

    AudioMidiLoop() : BasicLoop() {}

    template<typename SampleT, typename BufferPool>
    std::shared_ptr<AudioChannel<SampleT>>
    add_audio_channel(std::shared_ptr<BufferPool> const& buffer_pool,
                      size_t initial_max_buffers,
                      channel_mode_t mode,
                      bool thread_safe=true) {
        auto channel = std::make_shared<AudioChannel<SampleT>>(
            buffer_pool, initial_max_buffers, mode);
        auto fn = [=]() {
            mp_audio_channels.push_back(channel);
        };
        if (thread_safe) { exec_process_thread_command(fn); }
        else { fn(); }
        return channel;
    }

    template<typename SampleT>
    std::shared_ptr<AudioChannel<SampleT>> audio_channel(size_t idx, bool thread_safe = true) {
        std::shared_ptr<ChannelInterface> iface;
        if (thread_safe) {
        exec_process_thread_command([this, idx, &iface]() { iface = mp_audio_channels.at(idx); });
        } else { iface = mp_audio_channels.at(idx); }

        auto maybe_r = std::dynamic_pointer_cast<AudioChannel<SampleT>>(iface);
        if (!maybe_r) {
            throw std::runtime_error("Audio channel " + std::to_string(idx) + " is not of the requested channel type.");
        }
        return maybe_r;
    }

    template<typename TimeType, typename SizeType>
    std::shared_ptr<MidiChannel<TimeType, SizeType>> add_midi_channel(size_t data_size, channel_mode_t mode, bool thread_safe = true) {
        auto channel = std::make_shared<MidiChannel<TimeType, SizeType>>(data_size, mode);
        auto fn = [this, &channel]() {
            mp_midi_channels.push_back(channel);
        };
        if (thread_safe) { exec_process_thread_command(fn); }
        else { fn(); }

        return channel;
    }

    template<typename TimeType, typename SizeType>
    std::shared_ptr<MidiChannel<TimeType, SizeType>> midi_channel(size_t idx, bool thread_safe=true) {
        std::shared_ptr<ChannelInterface> iface;
        if (thread_safe) {
            exec_process_thread_command([this, idx, &iface]() { iface = mp_midi_channels.at(idx); });
        } else { iface = mp_midi_channels.at(idx); }

        auto maybe_r = std::dynamic_pointer_cast<MidiChannel<TimeType, SizeType>>(iface);
        if (!maybe_r) {
            throw std::runtime_error("Midi channel " + std::to_string(idx) + " is not of the requested channel type.");
        }
        return maybe_r;
    }

    size_t n_audio_channels (bool thread_safe=true) {
        size_t rval;
        if (thread_safe) { exec_process_thread_command( [this, &rval]() { rval = mp_audio_channels.size(); } ); }
        else { rval = mp_audio_channels.size(); }
        return rval;
    }

    size_t n_midi_channels (bool thread_safe=true) {
        size_t rval;
        if (thread_safe) { exec_process_thread_command( [this, &rval]() { rval = mp_midi_channels.size(); } ); }
        else { rval = mp_midi_channels.size(); }
        return rval;
    }

    void delete_audio_channel(size_t idx, bool thread_safe=true) {
        throw std::runtime_error("delete_audio_channel() not implemented");
    }

    void delete_midi_channel(size_t idx, bool thread_safe=true) {
        throw std::runtime_error("delete_midi_channel() not implemented");
    }

    void PROC_process_channels(
        loop_mode_t mode,
        size_t n_samples,
        size_t pos_before,
        size_t pos_after,
        size_t length_before,
        size_t length_after
        ) override {
        BasicLoop::PROC_process_channels(mode, n_samples, pos_before, pos_after, length_before, length_after);

        for(auto &aloop : mp_audio_channels) {
            aloop->PROC_process(mode, n_samples, pos_before, pos_after, length_before, length_after);
        }
        for(auto &mloop : mp_midi_channels) {
            mloop->PROC_process(mode, n_samples, pos_before, pos_after, length_before, length_after);
        }
    }

    std::optional<size_t> PROC_get_next_poi() const override {
        auto rval = BasicLoop::PROC_get_next_poi();

        auto merge = [&rval](std::optional<size_t> other) {
            if (other.has_value() &&
                (!rval.has_value() || other.value() < rval.value())) {
                    rval = other;
                }
        };

        for (auto &channel: mp_audio_channels) {
            merge(channel->PROC_get_next_poi(get_mode(), get_length(), get_position()));
        }
        for (auto &channel: mp_midi_channels) {
            merge(channel->PROC_get_next_poi(get_mode(), get_length(), get_position()));
        }

        return rval;
    }

    void PROC_handle_poi() override {
        BasicLoop::PROC_handle_poi();

        for (auto &channel: mp_audio_channels) {
            channel->PROC_handle_poi(get_mode(), get_length(), get_position());
        }
        for (auto &channel: mp_midi_channels) {
            channel->PROC_handle_poi(get_mode(), get_length(), get_position());
        }
    }
};