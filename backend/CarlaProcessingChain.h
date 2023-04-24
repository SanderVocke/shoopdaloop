#pragma once
#include "ProcessingChainInterface.h"
#include <lilv/lilv.h>

template<typename TimeType, typename SizeType>
class CarlaLV2ProcessingChain : public ProcessingChainInterface<TimeType, SizeType> {
public:
    enum class CarlaType {
        Rack,
        Patchbay2,
        Patchbay16
    };
    using SharedAudioPort = typename ProcessingChainInterface<TimeType, SizeType>::SharedAudioPort;
    using SharedMidiPort = typename ProcessingChainInterface<TimeType, SizeType>::SharedMidiPort;

private:
    static constexpr std::map<CarlaType, std::string> plugin_uris = {
        { CarlaType::Rack, "http://kxstudio.sf.net/carla/plugins/carlarack" },
        { CarlaType::Patchbay2, "http://kxstudio.sf.net/carla/plugins/carlapatchbay" },
        { CarlaType::Patchbay16, "http://kxstudio.sf.net/carla/plugins/carlapatchbay16" }
    };

    const LilvPlugin * m_plugin = nullptr;
    LilvInstance * m_instance = nullptr;
    std::vector<SharedAudioPort> m_input_audio_ports;
    std::vector<SharedAudioPort> m_output_audio_ports;
    std::vector<SharedMidiPort> m_input_midi_ports;
    std::vector<SharedMidiPort> m_output_midi_ports;

public:
    CarlaLV2ProcessingChain(
        std::unique_ptr<LilvWorld> const& lilv_world,
        CarlaType type,
        size_t sample_rate
    ) {
        const LilvPlugins* all_plugins = lilv_world_get_all_plugins(lilv_world.get());
        LilvNode *uri = lilv_new_uri(lilv_world.get(), plugin_uris[type]);
        m_plugin = lilv_plugins_get_by_uri(all_plugins, uri);
        if (!m_plugin) {
            throw std::runtime_error("Plugin " + plugin_uris[type] + " not found.");
        }
        m_instance = lilv_plugin_instantiate(m_plugin, (double)sample_rate, nullptr);
        if (!m_instance) {
            throw std::runtime_error("Plugin " + plugin_uris[type] + " failed to instantiate.");
        }
        size_t n_ports = lilv_plugin_get_num_ports(m_plugin);
    }

    void process(size_t frames) override {
        lilv_instance_activate(m_instance);
        lilv_instance_run(m_instance, frames);
        lilv_instance_deactivate(m_instance);
    }

    std::vector<SharedAudioPort> input_audio_ports() const override {
        return m_input_audio_ports;
    }

    std::vector<SharedAudioPort> output_audio_ports() const override {
        return m_output_audio_ports;
    }

    std::vector<SharedMidiPort> input_midi_ports() const override {
        return m_input_midi_ports;
    }

    std::vector<SharedMidiPort> output_midi_ports() const override {
        return m_output_midi_ports;
    }

    virtual bool is_freewheeling() const override {
        return false;
    }

    virtual void set_freewheeling(bool enabled) {}

    virtual ~CarlaLV2ProcessingChain() {
        if(m_instance) { lilv_instance_free(m_instance); m_instance = nullptr; }
    }

};