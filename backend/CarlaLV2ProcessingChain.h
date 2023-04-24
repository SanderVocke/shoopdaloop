#pragma once
#include "ProcessingChainInterface.h"
#include <lilv/lilv.h>
#include <lv2/urid/urid.h>
#include <lv2/options/options.h>
#include <iostream>

enum class CarlaProcessingChainType {
    Rack,
    Patchbay2,
    Patchbay16
};

template<typename TimeType, typename SizeType>
class CarlaLV2ProcessingChain : public ProcessingChainInterface<TimeType, SizeType> {
public:
    using SharedAudioPort = typename ProcessingChainInterface<TimeType, SizeType>::SharedAudioPort;
    using SharedMidiPort = typename ProcessingChainInterface<TimeType, SizeType>::SharedMidiPort;

private:
    const LilvPlugin * m_plugin = nullptr;
    LilvInstance * m_instance = nullptr;
    std::vector<SharedAudioPort> m_input_audio_ports;
    std::vector<SharedAudioPort> m_output_audio_ports;
    std::vector<SharedMidiPort> m_input_midi_ports;
    std::vector<SharedMidiPort> m_output_midi_ports;
    
    using UridMap = std::map<const char*, LV2_URID>;
    UridMap m_urid_map;

    static LV2_URID map_urid(LV2_URID_Map_Handle handle, const char* uri) {
        std::cout << "Map URI: " << uri << std::endl;
        auto urid_map = (UridMap*)handle;
        LV2_URID rval = urid_map->size();
        (*urid_map)[uri] = rval;
        return rval;
    }

public:
    CarlaLV2ProcessingChain(
        LilvWorld *lilv_world,
        CarlaProcessingChainType type,
        size_t sample_rate
    ) {
        static const std::map<CarlaProcessingChainType, std::string> plugin_uris = {
            { CarlaProcessingChainType::Rack, "http://kxstudio.sf.net/carla/plugins/carlarack" },
            { CarlaProcessingChainType::Patchbay2, "http://kxstudio.sf.net/carla/plugins/carlapatchbay" },
            { CarlaProcessingChainType::Patchbay16, "http://kxstudio.sf.net/carla/plugins/carlapatchbay16" }
        };

        const LilvPlugins* all_plugins = lilv_world_get_all_plugins(lilv_world);
        LilvNode *uri = lilv_new_uri(lilv_world, plugin_uris.at(type).c_str());
        
        m_plugin = lilv_plugins_get_by_uri(all_plugins, uri);
        if (!m_plugin) {
            throw std::runtime_error("Plugin " + plugin_uris.at(type) + " not found.");
        }

        // Options feature
        LV2_Options_Option end {
            LV2_OPTIONS_INSTANCE,
            0,
            0,
            0,
            0,
            nullptr
        };
        LV2_Feature options_feature {
            .URI = LV2_OPTIONS__options,
            .data = &end
        };

        // URID mapping feature
        m_urid_map.clear();
        LV2_URID_Map map {
            .handle = (LV2_URID_Map_Handle)&m_urid_map,
            .map = CarlaLV2ProcessingChain<TimeType, SizeType>::map_urid
        };
        LV2_Feature urid_feature {
            .URI = LV2_URID__map,
            .data = &map
        };

        std::vector<LV2_Feature*> features = { &options_feature, &urid_feature, nullptr };

        m_instance = lilv_plugin_instantiate(m_plugin, (double)sample_rate, features.data());
        if (!m_instance) {
            throw std::runtime_error("Plugin " + plugin_uris.at(type) + " failed to instantiate.");
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

    virtual void set_freewheeling(bool enabled) override {}

    virtual ~CarlaLV2ProcessingChain() {
        if(m_instance) { lilv_instance_free(m_instance); m_instance = nullptr; }
    }

};