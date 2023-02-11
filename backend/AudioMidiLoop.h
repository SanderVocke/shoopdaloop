#include "BasicLoop.h"
#include "SubloopInterface.h"
#include "AudioChannelSubloop.h"
#include "MidiChannelSubloop.h"

#include <vector>
#include <memory>

// Extend a BasicLoop with a set of audio and midi subloops.
class AudioMidiLoop : public BasicLoop {
    std::vector<std::shared_ptr<SubloopInterface>> m_audio_subloops;
    std::vector<std::shared_ptr<SubloopInterface>> m_midi_subloops;
public:

    AudioMidiLoop() : BasicLoop() {}

    template<typename SampleT, typename BufferPool>
    std::shared_ptr<AudioChannelSubloop<SampleT>>
    add_audio_channel(std::shared_ptr<BufferPool> const& buffer_pool,
                      size_t initial_max_buffers,
                      AudioOutputType output_type) {
        m_audio_subloops.push_back(std::make_shared<AudioChannelSubloop<SampleT>>(
            buffer_pool, initial_max_buffers, output_type));
        return m_audio_subloops.back();
    }

    template<typename SampleT>
    std::shared_ptr<AudioChannelSubloop<SampleT>> audio_channel(size_t idx) {
        auto maybe_r = std::dynamic_pointer_cast<AudioChannelSubloop<SampleT>>(m_audio_subloops.at(idx));
        if (!maybe_r) {
            throw std::runtime_error("Audio channel " + std::to_string(idx) + " is not of the requested channel type.");
        }
        return maybe_r;
    }

    template<typename TimeType, typename SizeType>
    std::shared_ptr<MidiChannelSubloop<TimeType, SizeType>> add_midi_channel(size_t data_size) {
        m_midi_subloops.push_back(std::make_shared<MidiChannelSubloop<TimeType, SizeType>>(data_size));
        return m_midi_subloops.back();
    }

    template<typename TimeType, typename SizeType>
    std::shared_ptr<MidiChannelSubloop<TimeType, SizeType>> midi_channel(size_t idx) {
        auto maybe_r = std::dynamic_pointer_cast<MidiChannelSubloop<TimeType, SizeType>>(m_midi_subloops.at(idx));
        if (!maybe_r) {
            throw std::runtime_error("Midi channel " + std::to_string(idx) + " is not of the requested channel type.");
        }
        return maybe_r;
    }

    void delete_audio_channel(size_t idx) {
        throw std::runtime_error("delete_audio_channel() not implemented");
    }

    void delete_midi_channel(size_t idx) {
        throw std::runtime_error("delete_midi_channel() not implemented");
    }

    void process_subloops(
        loop_state_t state_before,
        loop_state_t state_after,
        size_t n_samples,
        size_t pos_before,
        size_t pos_after,
        size_t length_before,
        size_t length_after
        ) override {
        BasicLoop::process_subloops(state_before, state_after, n_samples, pos_before, pos_after, length_before, length_after);

        for(auto &aloop : m_audio_subloops) {
            aloop->process(state_before, state_after, n_samples, pos_before, pos_after, length_before, length_after);
        }
        for(auto &mloop : m_midi_subloops) {
            mloop->process(state_before, state_after, n_samples, pos_before, pos_after, length_before, length_after);
        }
    }

    std::optional<size_t> get_next_poi() const override {
        auto rval = BasicLoop::get_next_poi();

        auto merge = [&](std::optional<size_t> other) {
            if (other.has_value() &&
                (!rval.has_value() || other.value() < rval.value())) {
                    rval = other;
                }
        };

        for (auto &channel: m_audio_subloops) {
            merge(channel->get_next_poi(get_state(), get_length(), get_position()));
        }
        for (auto &channel: m_midi_subloops) {
            merge(channel->get_next_poi(get_state(), get_length(), get_position()));
        }

        return rval;
    }

    void handle_poi() override {
        BasicLoop::handle_poi();

        for (auto &channel: m_audio_subloops) {
            channel->handle_poi(get_state(), get_length(), get_position());
        }
        for (auto &channel: m_midi_subloops) {
            channel->handle_poi(get_state(), get_length(), get_position());
        }
    }
};