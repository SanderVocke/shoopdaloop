#pragma once
#include "InternalLV2MidiOutputPort.h"
#include "ProcessingChainInterface.h"
#include "LoggingEnabled.h"
#include "random_string.h"

#include <cstring>
#include <memory>
#include <mutex>
#include <nlohmann/json_fwd.hpp>
#include <string>
#include <tuple>
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
#include <lv2_evbuf.h>
#include <lv2/midi/midi.h>
#include <lv2/atom/atom.h>
#include <lv2/state/state.h>
#include <lv2/event/event.h>
#include <lv2/event/event-helpers.h>
#include <iostream>
#include <dlfcn.h>
#include <thread>

#include <nlohmann/json.hpp>
#include <base64.hpp>

class LV2StateString {
    // map of key => (type, value)
    std::map<std::string, std::pair<std::string, std::vector<uint8_t>>> data;
    void * chain;
    LV2_URID (*map_urid)(LV2_URID_Map_Handle handle, const char* uri);
    const char* (*unmap_urid)(LV2_URID_Unmap_Handle handle, LV2_URID urid);

    LV2_URID do_map_urid(const char* uri) {
        return map_urid((LV2_URID_Map_Handle)chain, uri);
    }

    const char* do_unmap_urid(LV2_URID urid) {
        return unmap_urid((LV2_URID_Unmap_Handle)chain, urid);
    }
    
public:
    LV2StateString(
        decltype(chain) _chain,
        decltype(map_urid) _map_urid,
        decltype(unmap_urid) _unmap_urid) :
        chain(_chain), map_urid(_map_urid), unmap_urid(_unmap_urid) {}

    static LV2_State_Status store(LV2_State_Handle handle,
                                    uint32_t key,
                                    const void *value,
                                    size_t size,
                                    uint32_t type,
                                    uint32_t flags)
    {
        LV2StateString &instance = *((LV2StateString *)handle);
        std::string keystr(instance.do_unmap_urid(key));
        std::string typestr(instance.do_unmap_urid(type));
        return instance._store(keystr, value, size, typestr, flags);
    }

    LV2_State_Status _store(std::string key_uri,
                        const void* value,
                        size_t size,
                        std::string type_uri,
                        uint32_t flags)
    {
        if (!(flags & LV2_STATE_IS_POD) || !(flags & LV2_STATE_IS_PORTABLE)) {
            return LV2_STATE_ERR_BAD_FLAGS;
        }
        std::vector<uint8_t> _data(size);
        memcpy((void*)_data.data(), value, size);
        data[key_uri] = std::make_pair(
            type_uri,
            _data
        );
        return LV2_STATE_SUCCESS;
    }

    static const void* retrieve(LV2_State_Handle handle,
                                        uint32_t key,
                                        size_t *size,
                                        uint32_t *type,
                                        uint32_t *flags)
    {
        LV2StateString &instance = *((LV2StateString *)handle);
        std::string typestr;
        auto rval = instance._retrieve(
            instance.do_unmap_urid(key),
            *size,
            typestr,
            *flags);
        *type = instance.do_map_urid(typestr.c_str());
        return rval;
    }

    const void *_retrieve(std::string key_uri,
                        size_t &size_out,
                        std::string &type_uri_out,
                        uint32_t &flags_out)
    {
        auto f = data.find(key_uri);
        if (f != data.end()) {
            size_out = f->second.second.size();
            type_uri_out = f->second.first;
            flags_out = LV2_STATE_SUCCESS;
            return f->second.second.data();
        } else {
            flags_out = LV2_STATE_ERR_NO_PROPERTY;
            return nullptr;
        }
    }

    std::string serialize() const {
        nlohmann::json obj;
        for (auto &pair : data) {
            std::string str(pair.second.second.size(), '_');
            for (size_t i=0; i<pair.second.second.size(); i++) {str[i] = (char)pair.second.second[i];}
            obj[pair.first] = {
                    {"type", pair.second.first},
                    {"value", base64::to_base64(str)},
                };
        }
        return obj.dump(2);
    }

    void deserialize(std::string str) {
        decltype(data) _data;
        nlohmann::json obj = nlohmann::json::parse(str);
        for(auto it = obj.begin(); it != obj.end(); it++) {
            auto key = it.key();
            auto valuestr = base64::from_base64(it.value()["value"]);
            std::vector<uint8_t> value(valuestr.size());
            memcpy((void*)value.data(), (void*)valuestr.data(), valuestr.size());
            auto type = it.value()["type"];
            _data[key] = std::make_pair(type, value);
        }
        data = _data;
    }
};

template<typename TimeType, typename SizeType>
class CarlaLV2ProcessingChain : public ProcessingChainInterface<TimeType, SizeType>,
                                public ModuleLoggingEnabled {
public:
    using _InternalAudioPort = typename ProcessingChainInterface<TimeType, SizeType>::_InternalAudioPort;
    using SharedInternalAudioPort = typename ProcessingChainInterface<TimeType, SizeType>::SharedInternalAudioPort;
    using MidiOutputPort = InternalLV2MidiOutputPort;
    using SharedMidiOutputPort = std::shared_ptr<MidiOutputPort>;
    using MidiPort = typename ProcessingChainInterface<TimeType, SizeType>::MidiPort;
    using SharedMidiPort = typename ProcessingChainInterface<TimeType, SizeType>::SharedMidiPort;

    std::string log_module_name() const override {
        return "Backend.LV2";
    }

private:
    const LilvPlugin * m_plugin = nullptr;
    LilvInstance * m_instance = nullptr;
    LV2UI_Handle m_ui_handle = nullptr;
    LV2_External_UI_Widget * m_ui_widget = nullptr;
    std::recursive_mutex m_ui_mutex;
    std::thread m_ui_thread;
    bool m_visible = false;
    std::vector<const LilvPort*> m_audio_in_lilv_ports, m_audio_out_lilv_ports, m_midi_in_lilv_ports, m_midi_out_lilv_ports;
    std::vector<uint32_t> m_audio_in_port_indices, m_audio_out_port_indices, m_midi_in_port_indices, m_midi_out_port_indices;
    size_t m_internal_buffers_size;
    LV2_URID m_midi_event_type, m_atom_chunk_type, m_atom_sequence_type;
    LV2_External_UI_Host m_ui_host;
    LV2_State_Interface *m_state_iface;
    const LilvUI *m_ui;
    std::string m_plugin_uri;
    const size_t mc_midi_buf_capacities = 8192;
    LV2_URID_Map m_map_handle;
    LV2_URID_Unmap m_unmap_handle;
    std::string m_human_name;
    std::string m_unique_name;
    std::atomic<bool> m_active = false;
    std::atomic<bool> m_state_restore_active = false;

    const LV2UI_Descriptor * m_ui_descriptor = nullptr;

    std::vector<SharedInternalAudioPort> m_input_audio_ports;
    std::vector<SharedInternalAudioPort> m_output_audio_ports;
    std::vector<SharedMidiOutputPort> m_input_midi_ports;
    std::vector<SharedMidiPort> m_generic_input_midi_ports;
    std::vector<LV2_Evbuf*> m_output_midi_ports; // Note that these will not be used. Only there to satisfy the plugin.
    
    using UridMap = std::map<const char*, LV2_URID>;
    UridMap m_urid_map;

    static LV2_URID map_urid(LV2_URID_Map_Handle handle, const char* uri) {
        auto &instance = *((CarlaLV2ProcessingChain<TimeType, SizeType>*)handle);
        return instance.map_urid(uri);
    }

    LV2_URID map_urid(const char* uri) {
        auto f = std::find_if(m_urid_map.begin(), m_urid_map.end(), [&](auto &it) { return strcmp(it.first, uri) == 0; });
        if (f != m_urid_map.end()) {
            return f->second;
        }

        // Map new URID
        LV2_URID rval = m_urid_map.size()+1;
        m_urid_map[uri] = rval;
        return rval;
    }

    static const char* unmap_urid(LV2_URID_Unmap_Handle handle, LV2_URID urid) {
        auto &instance = *((CarlaLV2ProcessingChain<TimeType, SizeType>*)handle);
        return instance.unmap_urid(urid);
    }

    const char* unmap_urid(LV2_URID urid) {
        auto f = std::find_if(m_urid_map.begin(), m_urid_map.end(), [&](auto &it) { return it.second == urid; });
        return (f == m_urid_map.end()) ?
            nullptr :
            f->first;
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
        for(size_t i=0; i<m_input_midi_ports.size(); i++) {
            auto &midi_in = *m_input_midi_ports[i];
            auto &buf = midi_in.PROC_get_write_buffer(m_internal_buffers_size);
            lilv_instance_connect_port(m_instance, m_midi_in_port_indices[i], lv2_evbuf_get_buffer(midi_in.internal_evbuf()));
        }
        for(size_t i=0; i<m_output_midi_ports.size(); i++) {
            auto midi_out = m_output_midi_ports[i];
            lv2_evbuf_reset(midi_out, false);
            lilv_instance_connect_port(m_instance, m_midi_out_port_indices[i], lv2_evbuf_get_buffer(midi_out));
        }
    }

    void ui_closed_fn() {
        maybe_cleanup_ui();
    }

    void maybe_cleanup_ui() {
        {
            std::cout << "Start cleanup!\n";
            std::lock_guard<std::recursive_mutex> lock(m_ui_mutex);
            if (m_ui_handle) {
                m_ui_descriptor->cleanup(m_ui_handle);
            }
            m_ui_handle = nullptr;
            m_ui_widget = nullptr;
            m_visible = false;
            std::cout << "Finish cleanup!\n";
        }
    }

public:
    CarlaLV2ProcessingChain(
        LilvWorld *lilv_world,
        fx_chain_type_t type,
        size_t sample_rate,
        std::string human_name
    ) :
        m_internal_buffers_size(0),
        m_human_name(human_name),
        m_unique_name(human_name + "_" + random_string(6))
    {
        // URIs for the Carla plugins we want to support.
        static const std::map<fx_chain_type_t, std::string> plugin_uris = {
            { Carla_Rack, "http://kxstudio.sf.net/carla/plugins/carlarack" },
            { Carla_Patchbay, "http://kxstudio.sf.net/carla/plugins/carlapatchbay" },
            { Carla_Patchbay_16x, "http://kxstudio.sf.net/carla/plugins/carlapatchbay16" }
        };
        m_plugin_uri = plugin_uris.at(type);

        // Find our plugin.
        const LilvPlugins* all_plugins = lilv_world_get_all_plugins(lilv_world);
        LilvNode *uri = lilv_new_uri(lilv_world, m_plugin_uri.c_str());
        m_plugin = lilv_plugins_get_by_uri(all_plugins, uri);
        if (!m_plugin) {
            throw_error<std::runtime_error>("Plugin {} not found.", m_plugin_uri);
        }

        // Set up URID mapping feature and map some URIs we need.
        m_urid_map.clear();
        // Map URIs we know we will need
        m_midi_event_type = map_urid(LV2_MIDI__MidiEvent);
        m_atom_chunk_type = map_urid(LV2_ATOM__Chunk);
        m_atom_sequence_type = map_urid(LV2_ATOM__Sequence);

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
            midi_in_port_symbols.push_back("lv2_events_in");
            midi_out_port_symbols.push_back("lv2_events_out");

            for (auto const& sym : audio_in_port_symbols) {
                auto p = lilv_plugin_get_port_by_symbol(m_plugin, lilv_new_string(lilv_world, sym.c_str()));
                if (!p) { throw_error<std::runtime_error>("Could not find port '{}' on plugin.", sym); }
                m_audio_in_lilv_ports.push_back(p);
                m_audio_in_port_indices.push_back(lilv_port_get_index(m_plugin, p));
                auto internal = std::make_shared<_InternalAudioPort>(sym, PortDirection::Output, m_internal_buffers_size);
                m_input_audio_ports.push_back(internal);
            }
            for (auto const& sym : audio_out_port_symbols) {
                auto p = lilv_plugin_get_port_by_symbol(m_plugin, lilv_new_string(lilv_world, sym.c_str()));
                if (!p) { throw_error<std::runtime_error>("Could not find port '{}' on plugin.", sym); }
                m_audio_out_lilv_ports.push_back(p);
                m_audio_out_port_indices.push_back(lilv_port_get_index(m_plugin, p));
                auto internal = std::make_shared<_InternalAudioPort>(sym, PortDirection::Input, m_internal_buffers_size);
                m_output_audio_ports.push_back(internal);
            }
            for (auto const& sym : midi_in_port_symbols) {
                auto p = lilv_plugin_get_port_by_symbol(m_plugin, lilv_new_string(lilv_world, sym.c_str()));
                if (!p) { throw_error<std::runtime_error>("Could not find port '{}' on plugin.", sym); }
                m_midi_in_lilv_ports.push_back(p);
                m_midi_in_port_indices.push_back(lilv_port_get_index(m_plugin, p));
                auto internal = std::make_shared<InternalLV2MidiOutputPort>(sym, PortDirection::Output, mc_midi_buf_capacities, m_atom_chunk_type, m_atom_sequence_type, m_midi_event_type);
                m_input_midi_ports.push_back(internal);
                m_generic_input_midi_ports.push_back(std::static_pointer_cast<MidiPort>(internal));
            }
            for (auto const& sym : midi_out_port_symbols) {
                auto p = lilv_plugin_get_port_by_symbol(m_plugin, lilv_new_string(lilv_world, sym.c_str()));
                if (!p) { throw_error<std::runtime_error>("Could not find port '{}' on plugin.", sym); }
                m_midi_out_lilv_ports.push_back(p);
                m_midi_out_port_indices.push_back(lilv_port_get_index(m_plugin, p));
                auto internal = lv2_evbuf_new(mc_midi_buf_capacities, m_atom_chunk_type, m_atom_sequence_type);
                m_output_midi_ports.push_back(internal);
            }
        }

        // Set up the UI.
        {
            LilvUIs* uis = lilv_plugin_get_uis(m_plugin);
            if (lilv_uis_size(uis) != 1) {
                throw std::runtime_error("Expected 1 UI for plugin " + m_plugin_uri);
            }

            m_ui = lilv_uis_get(uis, lilv_uis_begin(uis));
            const LilvNode *external_ui_uri = lilv_new_uri(lilv_world, "http://kxstudio.sf.net/ns/lv2ext/external-ui#Widget");
            if (!lilv_ui_is_a(m_ui, external_ui_uri)) {
                throw std::runtime_error("Expected Carla UI to be of class 'external-ui'");
            }

            const LilvNode *ui_binary_uri = lilv_ui_get_binary_uri(m_ui);
            const char *ui_binary_path = lilv_node_get_path(ui_binary_uri, nullptr);
            void * ui_lib_handle = dlopen(ui_binary_path, RTLD_LAZY);
            LV2UI_DescriptorFunction _get_ui_descriptor = (LV2UI_DescriptorFunction)dlsym(ui_lib_handle, "lv2ui_descriptor");
            if (!_get_ui_descriptor) {
                throw std::runtime_error("Could not load UI descriptor entry point.");
            }

            m_ui_descriptor = _get_ui_descriptor(0);
            m_ui_host = {
                .ui_closed = static_ui_closed_fn,
                .plugin_human_id = m_human_name.c_str()
            };
        }

        // Start instantiation of the plugin.
        instantiate(sample_rate);
    }

    void instantiate(size_t sample_rate) {
        if (m_instance) {
            throw std::runtime_error("Cannot re-instantiate plugin");
        }

        // Instantiate asynchronously.
        std::thread instantiate_thread([this, sample_rate]() {
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

            m_map_handle = {
                .handle = (LV2_URID_Map_Handle)this,
                .map = CarlaLV2ProcessingChain<TimeType, SizeType>::map_urid
            };
            LV2_Feature urid_map_feature {
                .URI = LV2_URID__map,
                .data = &m_map_handle
            };

            m_unmap_handle = {
                .handle = (LV2_URID_Unmap_Handle)this,
                .unmap = CarlaLV2ProcessingChain<TimeType, SizeType>::unmap_urid
            };
            LV2_Feature urid_unmap_feature {
                .URI = LV2_URID__unmap,
                .data = &m_unmap_handle
            };

            std::vector<LV2_Feature*> features = {
                &options_feature,
                &urid_map_feature,
                &urid_unmap_feature,
                nullptr
            };

            // Make a plugin instance.
            auto instance = lilv_plugin_instantiate(m_plugin, (double)sample_rate, features.data());
            if (!instance) {
                throw std::runtime_error("Plugin " + m_plugin_uri + " failed to instantiate.");
            }

            // Set up state mgmt
            m_state_iface = (LV2_State_Interface*) lilv_instance_get_extension_data(instance, LV2_STATE__interface);

            m_instance = instance;
            // Connect ports.
            reconnect_ports();
        });
        instantiate_thread.detach();
    }

    bool visible() const {
        return m_visible;
    }

    void show() {
        if (!is_ready()) { return; }

        if (!m_ui_widget) {
            maybe_cleanup_ui();
            LV2_Feature instance_access_feature {
                .URI = LV2_INSTANCE_ACCESS_URI,
                .data = (void*) lilv_instance_get_handle(m_instance)
            };
            LV2_Feature external_ui_host_feature {
                .URI = LV2_EXTERNAL_UI__Host,
                .data = (void*)&m_ui_host
            };
            LV2UI_Widget ui_widget;
            const char *ui_bundle_path = lilv_node_get_path(lilv_ui_get_bundle_uri(m_ui), nullptr);
            const LV2_Feature* const ui_features[] = { &instance_access_feature, &external_ui_host_feature, nullptr };
            m_ui_handle = m_ui_descriptor->instantiate(
                m_ui_descriptor,
                m_plugin_uri.c_str(),
                ui_bundle_path,
                (LV2UI_Write_Function) static_ui_write_fn,
                (LV2UI_Controller) this,
                &ui_widget,
                ui_features
            );
            m_ui_widget = (LV2_External_UI_Widget*) ui_widget;
            if(!m_ui_widget) {
                throw std::runtime_error("Could not instantiate Carla UI.");
            }

            // Create and start UI thread
            // (also join the old UI thread if still alive)
            if (m_ui_thread.joinable()) {
                std::cout << "join!\n";
                m_ui_thread.join();
            }
            m_ui_thread = std::thread([this]() {
                while (true) {
                    auto t = std::chrono::high_resolution_clock::now();
                    {
                        std::lock_guard<std::recursive_mutex> lock(m_ui_mutex);
                        if (!m_ui_widget) { break; }
                        m_ui_widget->run(m_ui_widget);
                    }
                    while (t < std::chrono::high_resolution_clock::now()) {
                        t += std::chrono::milliseconds(30);
                    }
                    std::this_thread::sleep_until(t);
                }
            });
        }
        m_ui_widget->show(m_ui_widget);
        m_visible = true;
    }

    void hide() {
        std::cout << "Hide!\n";
        if(m_ui_widget) {
            m_ui_widget->hide(m_ui_widget);
        }
        m_visible = false;
    }

    void stop() {
        std::cout << "Carla instance stopping." << std::endl;
        maybe_cleanup_ui();
        if(m_ui_thread.joinable()) {
            m_ui_thread.join();
        }
        if(m_instance) { lilv_instance_free(m_instance); m_instance = nullptr; }
    }

    bool is_ready() const override { return m_instance != nullptr && !m_state_restore_active; }

    bool is_active() const override { return m_active; }

    void set_active(bool active) override { m_active = active; }

    void process(size_t frames) override {
        if(!m_active || !m_instance) {
            for (auto &port : m_output_audio_ports) {
                port->zero();
            }
            return;
        }

        if (frames > m_internal_buffers_size) {
            throw std::runtime_error("Carla processing chain: requesting to process more than buffer size.");
        }
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
        return m_generic_input_midi_ports;
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
            if (m_instance) {
                reconnect_ports();
            }
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

    void restore_state(std::string str) {
        while (!is_ready()) {
            std::this_thread::sleep_for(std::chrono::milliseconds(50));
        }
        if (!m_state_iface) {
            throw std::runtime_error("No state interface for Carla chain");
        }
        m_state_restore_active = true;
        std::thread restore_thread([this, str]() {
            try {
                LV2StateString s(this, CarlaLV2ProcessingChain<TimeType, SizeType>::map_urid, CarlaLV2ProcessingChain<TimeType, SizeType>::unmap_urid);
                static const LV2_Feature* features[] = { nullptr };
                s.deserialize(str);
                m_state_iface->restore(lilv_instance_get_handle(m_instance), LV2StateString::retrieve, (LV2_State_Handle)&s, LV2_STATE_IS_POD | LV2_STATE_IS_PORTABLE, features);
            } catch (const std::exception &exp) {
                std::cerr << "Failed to restore Carla state: " << exp.what();
            } catch (...) {
                std::cerr << "Failed to restore Carla state (unknown exception).";
            }

            m_state_restore_active = false;
        });
        restore_thread.detach();
    }

    std::string get_state() {
        while (!is_ready()) {
            std::this_thread::sleep_for(std::chrono::milliseconds(50));
        }
        if (!m_state_iface) {
            throw std::runtime_error("No state interface for Carla chain");
        }
        LV2StateString s(this, CarlaLV2ProcessingChain<TimeType, SizeType>::map_urid, CarlaLV2ProcessingChain<TimeType, SizeType>::unmap_urid);
        static const LV2_Feature* features[] = { nullptr };
        m_state_iface->save(lilv_instance_get_handle(m_instance), LV2StateString::store, (LV2_State_Handle)&s, LV2_STATE_IS_POD | LV2_STATE_IS_PORTABLE, features);
        return s.serialize();
    }
};