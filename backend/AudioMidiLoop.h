#include "BasicLoop.h"
#include "SubloopInterface.h"
#include "AudioChannelSubloop.h"
#include "MidiChannelSubloop.h"
#include "WithCommandQueue.h"

#include <vector>
#include <memory>

// Extend a BasicLoop with a set of audio and midi subloops.
class AudioMidiLoop : public BasicLoop {
    std::vector<std::shared_ptr<SubloopInterface>> mp_audio_subloops;
    std::vector<std::shared_ptr<SubloopInterface>> mp_midi_subloops;
public:

    AudioMidiLoop() : BasicLoop() {}

    template<typename SampleT, typename BufferPool>
    std::shared_ptr<AudioChannelSubloop<SampleT>>
    add_audio_channel(std::shared_ptr<BufferPool> const& buffer_pool,
                      size_t initial_max_buffers,
                      bool enabled,
                      bool thread_safe=true) {
        auto subloop = std::make_shared<AudioChannelSubloop<SampleT>>(
            buffer_pool, initial_max_buffers, enabled);
        auto fn = [=]() {
            mp_audio_subloops.push_back(subloop);
        };
        if (thread_safe) { exec_process_thread_command(fn); }
        else { fn(); }
        return subloop;
    }

    template<typename SampleT>
    std::shared_ptr<AudioChannelSubloop<SampleT>> audio_channel(size_t idx, bool thread_safe = true) {
        std::shared_ptr<SubloopInterface> iface;
        if (thread_safe) {
        exec_process_thread_command([this, idx, &iface]() { iface = mp_audio_subloops.at(idx); });
        } else { iface = mp_audio_subloops.at(idx); }

        auto maybe_r = std::dynamic_pointer_cast<AudioChannelSubloop<SampleT>>(iface);
        if (!maybe_r) {
            throw std::runtime_error("Audio channel " + std::to_string(idx) + " is not of the requested channel type.");
        }
        return maybe_r;
    }

    template<typename TimeType, typename SizeType>
    std::shared_ptr<MidiChannelSubloop<TimeType, SizeType>> add_midi_channel(size_t data_size, bool enabled, bool thread_safe = true) {
        auto subloop = std::make_shared<MidiChannelSubloop<TimeType, SizeType>>(data_size, enabled);
        auto fn = [this, &subloop]() {
            mp_midi_subloops.push_back(subloop);
        };
        if (thread_safe) { exec_process_thread_command(fn); }
        else { fn(); }

        return subloop;
    }

    template<typename TimeType, typename SizeType>
    std::shared_ptr<MidiChannelSubloop<TimeType, SizeType>> midi_channel(size_t idx, bool thread_safe=true) {
        std::shared_ptr<SubloopInterface> iface;
        if (thread_safe) {
            exec_process_thread_command([this, idx, &iface]() { iface = mp_midi_subloops.at(idx); });
        } else { iface = mp_midi_subloops.at(idx); }

        auto maybe_r = std::dynamic_pointer_cast<MidiChannelSubloop<TimeType, SizeType>>(iface);
        if (!maybe_r) {
            throw std::runtime_error("Midi channel " + std::to_string(idx) + " is not of the requested channel type.");
        }
        return maybe_r;
    }

    size_t n_audio_channels (bool thread_safe=true) {
        size_t rval;
        if (thread_safe) { exec_process_thread_command( [this, &rval]() { rval = mp_audio_subloops.size(); } ); }
        else { rval = mp_audio_subloops.size(); }
        return rval;
    }

    size_t n_midi_channels (bool thread_safe=true) {
        size_t rval;
        if (thread_safe) { exec_process_thread_command( [this, &rval]() { rval = mp_midi_subloops.size(); } ); }
        else { rval = mp_midi_subloops.size(); }
        return rval;
    }

    void delete_audio_channel(size_t idx, bool thread_safe=true) {
        throw std::runtime_error("delete_audio_channel() not implemented");
    }

    void delete_midi_channel(size_t idx, bool thread_safe=true) {
        throw std::runtime_error("delete_midi_channel() not implemented");
    }

    void PROC_process_subloops(
        loop_mode_t mode_before,
        loop_mode_t mode_after,
        size_t n_samples,
        size_t pos_before,
        size_t pos_after,
        size_t length_before,
        size_t length_after
        ) override {
        BasicLoop::PROC_process_subloops(mode_before, mode_after, n_samples, pos_before, pos_after, length_before, length_after);

        for(auto &aloop : mp_audio_subloops) {
            aloop->PROC_process(mode_before, mode_after, n_samples, pos_before, pos_after, length_before, length_after);
        }
        for(auto &mloop : mp_midi_subloops) {
            mloop->PROC_process(mode_before, mode_after, n_samples, pos_before, pos_after, length_before, length_after);
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

        for (auto &channel: mp_audio_subloops) {
            merge(channel->PROC_get_next_poi(get_mode(), get_length(), get_position()));
        }
        for (auto &channel: mp_midi_subloops) {
            merge(channel->PROC_get_next_poi(get_mode(), get_length(), get_position()));
        }

        return rval;
    }

    void PROC_handle_poi() override {
        BasicLoop::PROC_handle_poi();

        for (auto &channel: mp_audio_subloops) {
            channel->PROC_handle_poi(get_mode(), get_length(), get_position());
        }
        for (auto &channel: mp_midi_subloops) {
            channel->PROC_handle_poi(get_mode(), get_length(), get_position());
        }
    }
};