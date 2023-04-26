#pragma once
#include "ProcessingChainInterface.h"
#include "InternalAudioPort.h"

#include <string>
#include <types.h>

#include <atomic>
#include <chrono>
#include <lilv/lilv.h>
#include <lv2/core/lv2.h>
#include <lv2/urid/urid.h>
#include <lv2/options/options.h>
#include <lv2/ui/ui.h>
#include <lv2/instance-access/instance-access.h>
#include <lv2_external_ui.h>
#include <iostream>
#include <dlfcn.h>
#include <thread>


template<typename TimeType, typename SizeType>
class CarlaLV2ProcessingChain : public ProcessingChainInterface<TimeType, SizeType> {
public:
    using _InternalAudioPort = typename ProcessingChainInterface<TimeType, SizeType>::_InternalAudioPort;
    using SharedInternalAudioPort = typename ProcessingChainInterface<TimeType, SizeType>::SharedInternalAudioPort;
    using SharedMidiPort = typename ProcessingChainInterface<TimeType, SizeType>::SharedMidiPort;

private:
    const LilvPlugin * m_plugin = nullptr;
    LilvInstance * m_instance = nullptr;
    LV2UI_Handle m_ui_handle = nullptr;
    LV2_External_UI_Widget * m_ui_widget;
    std::atomic<bool> m_finish = false;
    std::thread m_ui_thread;
    bool m_visible = false;
    std::vector<const LilvPort*> m_audio_in_lilv_ports, m_audio_out_lilv_ports;
    std::vector<uint32_t> m_audio_in_port_indices, m_audio_out_port_indices;
    size_t m_internal_buffers_size;

    const LV2UI_Descriptor * m_ui_descriptor = nullptr;

    std::vector<SharedInternalAudioPort> m_input_audio_ports;
    std::vector<SharedInternalAudioPort> m_output_audio_ports;
    std::vector<SharedMidiPort> m_input_midi_ports;
    std::vector<SharedMidiPort> m_output_midi_ports;
    
    using UridMap = std::map<const char*, LV2_URID>;
    UridMap m_urid_map;

    static LV2_URID map_urid(LV2_URID_Map_Handle handle, const char* uri) {
        auto urid_map = (UridMap*)handle;
        LV2_URID rval = urid_map->size();
        (*urid_map)[uri] = rval;
        return rval;
    }

    static void static_ui_write_fn(LV2UI_Controller controller,
                                   uint32_t port_index,
                                   uint32_t buffer_size,
                                   uint32_t port_protocol,
                                   const void* buffer) 
    {
        auto &instance = *((CarlaLV2ProcessingChain<TimeType, SizeType>*)controller);
        instance.ui_write_fn(port_index, buffer_size, port_protocol, buffer);
    }

    void ui_write_fn(uint32_t port_index, uint32_t buffer_size, uint32_t port_protocol, const void* buffer) {
        std::cerr << "WARNING: UI write fn not implemented" << std::endl;
    }

    static void static_ui_closed_fn(LV2UI_Controller controller) {
        auto &instance = *((CarlaLV2ProcessingChain<TimeType, SizeType>*)controller);
        instance.ui_closed_fn();
    }

    void reconnect_ports() {
        for(size_t i=0; i<m_input_audio_ports.size(); i++) {
            lilv_instance_connect_port(m_instance, m_audio_in_port_indices[i], m_input_audio_ports[i]->PROC_get_buffer(m_internal_buffers_size));
        }
        for(size_t i=0; i<m_output_audio_ports.size(); i++) {
            lilv_instance_connect_port(m_instance, m_audio_out_port_indices[i], m_output_audio_ports[i]->PROC_get_buffer(m_internal_buffers_size));
        }
    }

    void ui_closed_fn() {
        stop();
    }

public:
    CarlaLV2ProcessingChain(
        LilvWorld *lilv_world,
        fx_chain_type_t type,
        size_t sample_rate
    ) :
        m_internal_buffers_size(0)
    {
        // URIs for the Carla plugins we want to support.
        static const std::map<fx_chain_type_t, std::string> plugin_uris = {
            { Carla_Rack, "http://kxstudio.sf.net/carla/plugins/carlarack" },
            { Carla_Patchbay, "http://kxstudio.sf.net/carla/plugins/carlapatchbay" },
            { Carla_Patchbay_16x, "http://kxstudio.sf.net/carla/plugins/carlapatchbay16" }
        };
        auto plugin_uri = plugin_uris.at(type);

        // Find our plugin.
        const LilvPlugins* all_plugins = lilv_world_get_all_plugins(lilv_world);
        LilvNode *uri = lilv_new_uri(lilv_world, plugin_uri.c_str());
        m_plugin = lilv_plugins_get_by_uri(all_plugins, uri);
        if (!m_plugin) {
            throw std::runtime_error("Plugin " + plugin_uri + " not found.");
        }

        // Set up required features.
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

        // Make a plugin instance.
        m_instance = lilv_plugin_instantiate(m_plugin, (double)sample_rate, features.data());
        if (!m_instance) {
            throw std::runtime_error("Plugin " + plugin_uri + " failed to instantiate.");
        }

        // Set up the plugin ports
        {
            std::vector<std::string> audio_in_port_symbols, audio_out_port_symbols, midi_in_port_symbols, midi_out_port_symbols;
            size_t n_audio =
                (type == Carla_Rack || type == Carla_Patchbay) ? 2 :
                (type == Carla_Patchbay_16x) ? 16 :
                0;
            for (size_t i=0; i<n_audio; i++) {
                audio_in_port_symbols.push_back("lv2_audio_in_" + std::to_string(i+1));
                audio_out_port_symbols.push_back("lv2_audio_out_" + std::to_string(i+1));
            }

            for (auto const& sym : audio_in_port_symbols) {
                auto p = lilv_plugin_get_port_by_symbol(m_plugin, lilv_new_string(lilv_world, sym.c_str()));
                if (!p) { throw std::runtime_error ("Could not find port '" + sym + "' on plugin."); }
                m_audio_in_lilv_ports.push_back(p);
                m_audio_in_port_indices.push_back(lilv_port_get_index(m_plugin, p));
                auto internal = std::make_shared<_InternalAudioPort>(sym, PortDirection::Input, m_internal_buffers_size);
                m_input_audio_ports.push_back(internal);
            }
            for (auto const& sym : audio_out_port_symbols) {
                auto p = lilv_plugin_get_port_by_symbol(m_plugin, lilv_new_string(lilv_world, sym.c_str()));
                if (!p) { throw std::runtime_error ("Could not find port '" + sym + "' on plugin."); }
                m_audio_out_lilv_ports.push_back(p);
                m_audio_out_port_indices.push_back(lilv_port_get_index(m_plugin, p));
                auto internal = std::make_shared<_InternalAudioPort>(sym, PortDirection::Output, m_internal_buffers_size);
                m_output_audio_ports.push_back(internal);
            }
            reconnect_ports();
        }

        // Set up the UI.
        {
            LilvUIs* uis = lilv_plugin_get_uis(m_plugin);
            if (lilv_uis_size(uis) != 1) {
                throw std::runtime_error("Expected 1 UI for plugin " + plugin_uri);
            }

            const LilvUI *ui = lilv_uis_get(uis, lilv_uis_begin(uis));
            const LilvNode *external_ui_uri = lilv_new_uri(lilv_world, "http://kxstudio.sf.net/ns/lv2ext/external-ui#Widget");
            if (!lilv_ui_is_a(ui, external_ui_uri)) {
                throw std::runtime_error("Expected Carla UI to be of class 'external-ui'");
            }

            const LilvNode *ui_binary_uri = lilv_ui_get_binary_uri(ui);
            const char *ui_binary_path = lilv_node_get_path(ui_binary_uri, nullptr);
            void * ui_lib_handle = dlopen(ui_binary_path, RTLD_LAZY);
            LV2UI_DescriptorFunction _get_ui_descriptor = (LV2UI_DescriptorFunction)dlsym(ui_lib_handle, "lv2ui_descriptor");
            if (!_get_ui_descriptor) {
                throw std::runtime_error("Could not load UI descriptor entry point.");
            }

            m_ui_descriptor = _get_ui_descriptor(0);
            LV2_Feature instance_access_feature {
                .URI = LV2_INSTANCE_ACCESS_URI,
                .data = (void*) lilv_instance_get_handle(m_instance)
            };
            LV2_External_UI_Host ui_host {
                .ui_closed = static_ui_closed_fn,
                .plugin_human_id = "shoopdaloop"
            };
            LV2_Feature external_ui_host_feature {
                .URI = LV2_EXTERNAL_UI__Host,
                .data = (void*)&ui_host
            };
            LV2UI_Widget ui_widget;
            const char *ui_bundle_path = lilv_node_get_path(lilv_ui_get_bundle_uri(ui), nullptr);
            const LV2_Feature* const ui_features[] = { &instance_access_feature, &external_ui_host_feature, nullptr };
            m_ui_handle = m_ui_descriptor->instantiate(
                m_ui_descriptor,
                plugin_uri.c_str(),
                ui_bundle_path,
                (LV2UI_Write_Function) static_ui_write_fn,
                (LV2UI_Controller) this,
                &ui_widget,
                ui_features
            );
            m_ui_widget = (LV2_External_UI_Widget*) ui_widget;
            if(!m_ui_handle) {
                throw std::runtime_error("Could not instantiate Carla UI.");
            }
        }

        // Create and start UI thread
        m_ui_thread = std::thread([this]() {
            while(!m_finish) {
                auto t = std::chrono::high_resolution_clock::now();
                m_ui_widget->run(m_ui_widget);
                while (t < std::chrono::high_resolution_clock::now()) {
                    t += std::chrono::milliseconds(30);
                }
                std::this_thread::sleep_until(t);
            }
        });
    }

    bool visible() {
        return m_visible;
    }

    void show() {
        m_ui_widget->show(m_ui_widget);
        m_visible = true;
    }

    void hide() {
        m_ui_widget->hide(m_ui_widget);
        m_visible = false;
    }

    void stop() {
        std::cout << "Carla instance stopping." << std::endl;
        m_finish = true;
        if(m_ui_thread.joinable()) {
            m_ui_thread.join();
        }
        if(m_instance) { lilv_instance_free(m_instance); m_instance = nullptr; }
    }

    bool running() const { return !m_finish && m_ui_thread.joinable(); }

    void process(size_t frames) override {
        lilv_instance_activate(m_instance);
        lilv_instance_run(m_instance, frames);
        lilv_instance_deactivate(m_instance);
    }

    std::vector<SharedInternalAudioPort> const& input_audio_ports() const override {
        return m_input_audio_ports;
    }

    std::vector<SharedInternalAudioPort> const& output_audio_ports() const override {
        return m_output_audio_ports;
    }

    std::vector<SharedMidiPort> const& input_midi_ports() const override {
        return m_input_midi_ports;
    }

    std::vector<SharedMidiPort> const& output_midi_ports() const override {
        return m_output_midi_ports;
    }

    bool is_freewheeling() const override {
        return false;
    }

    void set_freewheeling(bool enabled) override {}

    void ensure_buffers(size_t size) {
        if (size > m_internal_buffers_size) {
            for (auto &port : m_input_audio_ports) {
                port->reallocate_buffer(size);
            }
            for (auto &port : m_output_audio_ports) {
                port->reallocate_buffer(size);
            }
            reconnect_ports();
            m_internal_buffers_size = size;
        }
    }

    size_t buffers_size() const {
        throw std::runtime_error("Buffer size getting not yet implemented");
    }

    virtual ~CarlaLV2ProcessingChain() {
        std::cout << "Destroying Carla processing chain." << std::endl;
        stop();
    }

};