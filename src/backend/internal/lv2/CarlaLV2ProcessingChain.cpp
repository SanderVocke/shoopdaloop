#include "CarlaLV2ProcessingChain.h"
#include "random_string.h"
#include <cstring>
#include <memory>
#include <mutex>
#include <nlohmann/json_fwd.hpp>
#include <tuple>
#include <atomic>
#include <chrono>
#include <iostream>
#include <thread>
#include <nlohmann/json.hpp>
#include <base64.hpp>
#include "LoadDynamicLibrary.h"
#include "types.h"

#ifdef _WIN32
#undef min
#undef max
#endif
namespace carla_constants {
    constexpr uint32_t max_buffer_size = 8192;
    constexpr uint32_t min_buffer_size = 1;
}

LV2StateString::LV2StateString(decltype(chain) _chain,
                               decltype(map_urid) _map_urid,
                               decltype(unmap_urid) _unmap_urid)
    : chain(_chain), map_urid(_map_urid), unmap_urid(_unmap_urid) {}

LV2_URID LV2StateString::do_map_urid(const char *uri) {
    return map_urid((LV2_URID_Map_Handle)chain, uri);
}

const char *LV2StateString::do_unmap_urid(LV2_URID urid) {
    return unmap_urid((LV2_URID_Unmap_Handle)chain, urid);
}

LV2_State_Status LV2StateString::store(LV2_State_Handle handle, uint32_t key,
                                       const void *value, size_t size,
                                       uint32_t type, uint32_t flags) {
    LV2StateString &instance = *((LV2StateString *)handle);
    std::string keystr(instance.do_unmap_urid(key));
    std::string typestr(instance.do_unmap_urid(type));
    return instance._store(keystr, value, size, typestr, flags);
}

LV2_State_Status LV2StateString::_store(std::string key_uri, const void *value,
                                        size_t size, std::string type_uri,
                                        uint32_t flags) {
    if (!(flags & LV2_STATE_IS_POD) || !(flags & LV2_STATE_IS_PORTABLE)) {
        return LV2_STATE_ERR_BAD_FLAGS;
    }
    std::vector<uint8_t> _data(size);
    memcpy((void *)_data.data(), value, size);
    data[key_uri] = std::make_pair(type_uri, _data);
    return LV2_STATE_SUCCESS;
}

const void *LV2StateString::retrieve(LV2_State_Handle handle, uint32_t key,
                                     size_t *size, uint32_t *type,
                                     uint32_t *flags) {
    LV2StateString &instance = *((LV2StateString *)handle);
    std::string typestr;
    auto rval =
        instance._retrieve(instance.do_unmap_urid(key), *size, typestr, *flags);
    *type = instance.do_map_urid(typestr.c_str());
    return rval;
}

const void *LV2StateString::_retrieve(std::string key_uri, size_t &size_out,
                                      std::string &type_uri_out,
                                      uint32_t &flags_out) {
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

std::string LV2StateString::serialize() const {
    nlohmann::json obj;
    for (auto &pair : data) {
        std::string str(pair.second.second.size(), '_');
        for (uint32_t i = 0; i < pair.second.second.size(); i++) {
            str[i] = (char)pair.second.second[i];
        }
        obj[pair.first] = {
            {"type", pair.second.first},
            {"value", base64::to_base64(str)},
        };
    }
    return obj.dump(2);
}

void LV2StateString::deserialize(std::string str) {
    decltype(data) _data;
    nlohmann::json obj = nlohmann::json::parse(str);
    for (auto it = obj.begin(); it != obj.end(); it++) {
        auto key = it.key();
        auto valuestr = base64::from_base64(it.value()["value"]);
        std::vector<uint8_t> value(valuestr.size());
        memcpy((void *)value.data(), (void *)valuestr.data(), valuestr.size());
        auto type = it.value()["type"];
        _data[key] = std::make_pair(type, value);
    }
    data = _data;
}

template <typename TimeType, typename SizeType>
LV2_URID CarlaLV2ProcessingChain<TimeType, SizeType>::map_urid(
    LV2_URID_Map_Handle handle, const char *uri) {
    auto &instance = *((CarlaLV2ProcessingChain<TimeType, SizeType> *)handle);
    return instance.map_urid(uri);
}

template <typename TimeType, typename SizeType>
LV2_URID
CarlaLV2ProcessingChain<TimeType, SizeType>::map_urid(const char *uri) {
    auto f = std::find_if(m_urid_map.begin(), m_urid_map.end(),
                          [&](auto &it) { return strcmp(it.first, uri) == 0; });
    if (f != m_urid_map.end()) {
        return f->second;
    }

    // Map new URID
    LV2_URID rval = m_urid_map.size() + 1;
    m_urid_map[uri] = rval;
    return rval;
}

template <typename TimeType, typename SizeType>
const char *CarlaLV2ProcessingChain<TimeType, SizeType>::unmap_urid(
    LV2_URID_Unmap_Handle handle, LV2_URID urid) {
    auto &instance = *((CarlaLV2ProcessingChain<TimeType, SizeType> *)handle);
    return instance.unmap_urid(urid);
}

template <typename TimeType, typename SizeType>
const char *
CarlaLV2ProcessingChain<TimeType, SizeType>::unmap_urid(LV2_URID urid) {
    auto f = std::find_if(m_urid_map.begin(), m_urid_map.end(),
                          [&](auto &it) { return it.second == urid; });
    return (f == m_urid_map.end()) ? nullptr : f->first;
}

template <typename TimeType, typename SizeType>
void CarlaLV2ProcessingChain<TimeType, SizeType>::static_ui_write_fn(
    LV2UI_Controller controller, uint32_t port_index, uint32_t buffer_size,
    uint32_t port_protocol, const void *buffer) {
    auto &instance =
        *((CarlaLV2ProcessingChain<TimeType, SizeType> *)controller);
    instance.ui_write_fn(port_index, buffer_size, port_protocol, buffer);
}

template <typename TimeType, typename SizeType>
void CarlaLV2ProcessingChain<TimeType, SizeType>::ui_write_fn(
    uint32_t port_index, uint32_t buffer_size, uint32_t port_protocol,
    const void *buffer) {
    std::cerr << "WARNING: UI write fn not implemented" << std::endl;
}

template <typename TimeType, typename SizeType>
void CarlaLV2ProcessingChain<TimeType, SizeType>::static_ui_closed_fn(
    LV2UI_Controller controller) {
    auto &instance =
        *((CarlaLV2ProcessingChain<TimeType, SizeType> *)controller);
    instance.ui_closed_fn();
}

template <typename TimeType, typename SizeType>
void CarlaLV2ProcessingChain<TimeType, SizeType>::reconnect_ports() {
    for (uint32_t i = 0; i < m_input_audio_ports.size(); i++) {
        lilv_instance_connect_port(
            m_instance, m_audio_in_port_indices[i],
            m_input_audio_ports[i]->PROC_get_buffer(m_internal_buffers_size));
    }
    for (uint32_t i = 0; i < m_output_audio_ports.size(); i++) {
        lilv_instance_connect_port(
            m_instance, m_audio_out_port_indices[i],
            m_output_audio_ports[i]->PROC_get_buffer(m_internal_buffers_size));
    }
    for (uint32_t i = 0; i < m_input_midi_ports.size(); i++) {
        auto &midi_in = *m_input_midi_ports[i];
        auto buf = midi_in.PROC_get_write_data_into_port_buffer(m_internal_buffers_size);
        lilv_instance_connect_port(
            m_instance, m_midi_in_port_indices[i],
            lv2_evbuf_get_buffer(midi_in.internal_evbuf()));
    }
    for (uint32_t i = 0; i < m_output_midi_ports.size(); i++) {
        auto midi_out = m_output_midi_ports[i];
        lv2_evbuf_reset(midi_out, false);
        lilv_instance_connect_port(m_instance, m_midi_out_port_indices[i],
                                   lv2_evbuf_get_buffer(midi_out));
    }
}

template <typename TimeType, typename SizeType>
void CarlaLV2ProcessingChain<TimeType, SizeType>::ui_closed_fn() {
    maybe_cleanup_ui();
}

template <typename TimeType, typename SizeType>
void CarlaLV2ProcessingChain<TimeType, SizeType>::maybe_cleanup_ui() {
    {
        log<log_level_debug>("Cleaning up Carla UI.");
        std::lock_guard<std::recursive_mutex> lock(m_ui_mutex);
        if (m_ui_handle) {
            m_ui_descriptor->cleanup(m_ui_handle);
        }
        m_ui_handle = nullptr;
        m_ui_widget = nullptr;
        m_visible = false;
    }
}

template <typename TimeType, typename SizeType>
CarlaLV2ProcessingChain<TimeType, SizeType>::CarlaLV2ProcessingChain(
    LilvWorld *lilv_world, shoop_fx_chain_type_t type, uint32_t sample_rate, uint32_t buffer_size,
    std::string human_name, shoop_shared_ptr<typename AudioPort<shoop_types::audio_sample_t>::BufferPool> maybe_buffer_pool)
    : m_internal_buffers_size(carla_constants::max_buffer_size), m_human_name(human_name),
      m_unique_name(human_name + "_" + random_string(6)) {

    // URIs for the Carla plugins we want to support.
    static const std::map<shoop_fx_chain_type_t, std::string> plugin_uris = {
        {Carla_Rack, "http://kxstudio.sf.net/carla/plugins/carlarack"},
        {Carla_Patchbay, "http://kxstudio.sf.net/carla/plugins/carlapatchbay"},
        {Carla_Patchbay_16x,
         "http://kxstudio.sf.net/carla/plugins/carlapatchbay16"}};
    m_plugin_uri = plugin_uris.at(type);

    // Find our plugin.
    const LilvPlugins *all_plugins = lilv_world_get_all_plugins(lilv_world);
    LilvNode *uri = lilv_new_uri(lilv_world, m_plugin_uri.c_str());
    m_plugin = lilv_plugins_get_by_uri(all_plugins, uri);
    if (!m_plugin) {
        #ifdef _WIN32
        throw_error<std::runtime_error>("Plugin {} not found. Ensure you have Carla installed as Carla.lv2 in your LV2_PATH. Default plugins search path is C:\\Program Files\\Common Files\\LV2.", m_plugin_uri);
        #else
        throw_error<std::runtime_error>("Plugin {} not found.", m_plugin_uri);
        #endif
    }

    // Set up URID mapping feature and map some URIs we need.
    m_urid_map.clear();
    // Map URIs we know we will need.
    m_midi_event_type = map_urid(LV2_MIDI__MidiEvent);
    m_atom_chunk_type = map_urid(LV2_ATOM__Chunk);
    m_atom_sequence_type = map_urid(LV2_ATOM__Sequence);
    m_atom_int_type = map_urid(LV2_ATOM__Int);
    m_maxbufferuint32_type = map_urid(LV2_BUF_SIZE__maxBlockLength);
    m_minbufferuint32_type = map_urid(LV2_BUF_SIZE__minBlockLength);
    m_nominalbufferuint32_type = map_urid(LV2_BUF_SIZE__nominalBlockLength);

    // Set up the plugin ports
    {
        std::vector<std::string> audio_in_port_symbols, audio_out_port_symbols,
            midi_in_port_symbols, midi_out_port_symbols;
        uint32_t n_audio = (type == Carla_Rack || type == Carla_Patchbay) ? 2
                         : (type == Carla_Patchbay_16x)                 ? 16
                                                                        : 0;
        for (uint32_t i = 0; i < n_audio; i++) {
            audio_in_port_symbols.push_back("lv2_audio_in_" +
                                            std::to_string(i + 1));
            audio_out_port_symbols.push_back("lv2_audio_out_" +
                                             std::to_string(i + 1));
        }
        midi_in_port_symbols.push_back("lv2_events_in");
        midi_out_port_symbols.push_back("lv2_events_out");

        for (auto const &sym : audio_in_port_symbols) {
            auto p = lilv_plugin_get_port_by_symbol(
                m_plugin, lilv_new_string(lilv_world, sym.c_str()));
            if (!p) {
                throw_error<std::runtime_error>(
                    "Could not find port '{}' on plugin.", sym);
            }
            m_audio_in_lilv_ports.push_back(p);
            m_audio_in_port_indices.push_back(lilv_port_get_index(m_plugin, p));
            auto internal = shoop_make_shared<_InternalAudioPort>(
                sym, m_internal_buffers_size, nullptr);
            m_input_audio_ports.push_back(internal);
        }
        for (auto const &sym : audio_out_port_symbols) {
            auto p = lilv_plugin_get_port_by_symbol(
                m_plugin, lilv_new_string(lilv_world, sym.c_str()));
            if (!p) {
                throw_error<std::runtime_error>(
                    "Could not find port '{}' on plugin.", sym);
            }
            m_audio_out_lilv_ports.push_back(p);
            m_audio_out_port_indices.push_back(
                lilv_port_get_index(m_plugin, p));
            // This port gets a buffer pool to create a ringbuffer, because other downstream
            // channels may want to grab it
            auto internal = shoop_make_shared<_InternalAudioPort>(
                sym, m_internal_buffers_size, maybe_buffer_pool);
            m_output_audio_ports.push_back(internal);
        }
        for (auto const &sym : midi_in_port_symbols) {
            auto p = lilv_plugin_get_port_by_symbol(
                m_plugin, lilv_new_string(lilv_world, sym.c_str()));
            if (!p) {
                throw_error<std::runtime_error>(
                    "Could not find port '{}' on plugin.", sym);
            }
            m_midi_in_lilv_ports.push_back(p);
            m_midi_in_port_indices.push_back(lilv_port_get_index(m_plugin, p));
            auto internal = shoop_make_shared<InternalLV2MidiOutputPort>(
                sym, shoop_port_direction_t::ShoopPortDirection_Output, mc_midi_buf_capacities,
                m_atom_chunk_type, m_atom_sequence_type, m_midi_event_type);
            m_input_midi_ports.push_back(internal);
            m_generic_input_midi_ports.push_back(
                shoop_static_pointer_cast<MidiPort>(internal));
        }
        for (auto const &sym : midi_out_port_symbols) {
            auto p = lilv_plugin_get_port_by_symbol(
                m_plugin, lilv_new_string(lilv_world, sym.c_str()));
            if (!p) {
                throw_error<std::runtime_error>(
                    "Could not find port '{}' on plugin.", sym);
            }
            m_midi_out_lilv_ports.push_back(p);
            m_midi_out_port_indices.push_back(lilv_port_get_index(m_plugin, p));
            auto internal =
                lv2_evbuf_new(mc_midi_buf_capacities, m_atom_chunk_type,
                              m_atom_sequence_type);
            m_output_midi_ports.push_back(internal);
        }
    }

    // Set up the UI.
    {
        LilvUIs *uis = lilv_plugin_get_uis(m_plugin);
        if (lilv_uis_size(uis) != 1) {
            throw std::runtime_error("Expected 1 UI for plugin " +
                                     m_plugin_uri);
        }

        m_ui = lilv_uis_get(uis, lilv_uis_begin(uis));
        const LilvNode *external_ui_uri = lilv_new_uri(
            lilv_world, "http://kxstudio.sf.net/ns/lv2ext/external-ui#Widget");
        if (!lilv_ui_is_a(m_ui, external_ui_uri)) {
            throw std::runtime_error(
                "Expected Carla UI to be of class 'external-ui'");
        }

        const LilvNode *ui_binary_uri = lilv_ui_get_binary_uri(m_ui);
        const char *ui_binary_path = lilv_node_get_path(ui_binary_uri, nullptr);
        log<log_level_debug>("Loading UI library: {}",
                                      ui_binary_path);
        _dylib_handle ui_lib_handle = load_dylib(ui_binary_path);
        throw_if_dylib_error();
        LV2UI_DescriptorFunction _get_ui_descriptor =
            (LV2UI_DescriptorFunction)get_dylib_fn(ui_lib_handle, "lv2ui_descriptor");
        if (!_get_ui_descriptor) {
            throw_error<std::runtime_error>(
                "Could not load UI descriptor entry point (lv2ui_descriptor symbol in {}).", ui_binary_path);
        }

        m_ui_descriptor = _get_ui_descriptor(0);
        m_ui_host = {.ui_closed = static_ui_closed_fn,
                     .plugin_human_id = m_human_name.c_str()};
    }

    // Start instantiation of the plugin.
    instantiate(sample_rate, buffer_size);
}

template <typename TimeType, typename SizeType>
void CarlaLV2ProcessingChain<TimeType, SizeType>::instantiate(
    uint32_t sample_rate, uint32_t buffer_size) {
    if (m_instance) {
        throw std::runtime_error("Cannot re-instantiate Carla chain");
    }

    // Instantiate asynchronously.
    std::thread instantiate_thread([this, sample_rate, buffer_size]() {
        // Set up required features.
        // Options feature with buffer size
        LV2_Options_Option options[] = {
            {LV2_OPTIONS_INSTANCE, 0, m_maxbufferuint32_type, sizeof(decltype(carla_constants::max_buffer_size)), m_atom_int_type, &carla_constants::max_buffer_size },
            {LV2_OPTIONS_INSTANCE, 0, m_minbufferuint32_type, sizeof(decltype(carla_constants::min_buffer_size)), m_atom_int_type, &carla_constants::min_buffer_size },
            {LV2_OPTIONS_INSTANCE, 0, m_nominalbufferuint32_type, sizeof(decltype(buffer_size)), m_atom_int_type, &buffer_size},
            {LV2_OPTIONS_INSTANCE, 0, 0, 0, 0, nullptr}
        };
        LV2_Feature options_feature{.URI = LV2_OPTIONS__options, .data = options};

        m_map_handle = {
            .handle = (LV2_URID_Map_Handle)this,
            .map = CarlaLV2ProcessingChain<TimeType, SizeType>::map_urid};
        LV2_Feature urid_map_feature{.URI = LV2_URID__map,
                                     .data = &m_map_handle};

        m_unmap_handle = {
            .handle = (LV2_URID_Unmap_Handle)this,
            .unmap = CarlaLV2ProcessingChain<TimeType, SizeType>::unmap_urid};
        LV2_Feature urid_unmap_feature{.URI = LV2_URID__unmap,
                                       .data = &m_unmap_handle};

        std::vector<LV2_Feature *> features = {
            &options_feature, &urid_map_feature, &urid_unmap_feature, nullptr};

        // Make a plugin instance.
        auto instance = lilv_plugin_instantiate(m_plugin, (double)sample_rate,
                                                features.data());

        if (!instance) {
            throw std::runtime_error("Plugin " + m_plugin_uri +
                                     " failed to instantiate.");
        }

        // Set up state mgmt
        m_state_iface = (LV2_State_Interface *)lilv_instance_get_extension_data(
            instance, LV2_STATE__interface);

        m_instance = instance;
        // Connect ports.
        reconnect_ports();
    });
    instantiate_thread.detach();
}

template <typename TimeType, typename SizeType>
bool CarlaLV2ProcessingChain<TimeType, SizeType>::visible() const {
    return m_visible;
}

template <typename TimeType, typename SizeType>
void CarlaLV2ProcessingChain<TimeType, SizeType>::show() {
    if (!is_ready() || m_busy_making_visible) {
        log<log_level_debug>("Not ready or already starting to show.");
        return;
    }

    log<log_level_debug>("Showing Carla UI.");

    m_busy_making_visible = true;
    if (!m_ui_widget) {
        // Create and start UI thread
        // (also join the old UI thread if still alive)
        if (m_ui_thread.joinable()) {
            log<log_level_debug>("Waiting for UI thread to join.");
            m_ui_thread.join();
            log<log_level_debug>("Joined UI thread.");
        }
        m_ui_thread = std::thread([this]() {
            log<log_level_debug>("UI thread started.");
            maybe_cleanup_ui();
            LV2_Feature instance_access_feature{
                .URI = LV2_INSTANCE_ACCESS_URI,
                .data = (void *)lilv_instance_get_handle(m_instance)};
            LV2_Feature external_ui_host_feature{.URI = LV2_EXTERNAL_UI__Host,
                                                .data = (void *)&m_ui_host};
            LV2UI_Widget ui_widget;
            const char *ui_bundle_path =
                lilv_node_get_path(lilv_ui_get_bundle_uri(m_ui), nullptr);
            const LV2_Feature *const ui_features[] = {
                &instance_access_feature, &external_ui_host_feature, nullptr};
            m_ui_handle = m_ui_descriptor->instantiate(
                m_ui_descriptor, m_plugin_uri.c_str(), ui_bundle_path,
                (LV2UI_Write_Function)static_ui_write_fn, (LV2UI_Controller)this,
                &ui_widget, ui_features);
            m_ui_widget = (LV2_External_UI_Widget *)ui_widget;
            if (!m_ui_widget) {
                throw std::runtime_error("Could not instantiate Carla UI.");
            }
            m_ui_widget->show(m_ui_widget);
            m_visible = true;
            m_busy_making_visible = false;
            log<log_level_debug>("UI thread reached update loop.");
            while (true) {
                auto t = std::chrono::high_resolution_clock::now();
                {
                    std::lock_guard<std::recursive_mutex> lock(m_ui_mutex);
                    if (!m_ui_widget) {
                        break;
                    }
                    m_ui_widget->run(m_ui_widget);
                }
                while (t < std::chrono::high_resolution_clock::now()) {
                    t += std::chrono::milliseconds(30);
                }
                std::this_thread::sleep_until(t);
            }
        });
    }
}

template <typename TimeType, typename SizeType>
void CarlaLV2ProcessingChain<TimeType, SizeType>::hide() {
    log<log_level_debug>("Hiding Carla UI.");
    if (m_ui_widget) {
        m_ui_widget->hide(m_ui_widget);
    }
    maybe_cleanup_ui();
    if (m_ui_thread.joinable()) {
        log<log_level_debug>("Waiting for UI thread to join.");
        m_ui_thread.join();
        log<log_level_debug>("Joined UI thread.");
    }
    m_busy_making_visible = false;
    m_visible = false;
}

template <typename TimeType, typename SizeType>
void CarlaLV2ProcessingChain<TimeType, SizeType>::stop() {
    log<log_level_debug>("Stopping Carla.");
    maybe_cleanup_ui();
    if (m_ui_thread.joinable()) {
        m_ui_thread.join();
    }
    if (m_instance) {
        lilv_instance_free(m_instance);
        m_instance = nullptr;
    }
}

template <typename TimeType, typename SizeType>
bool CarlaLV2ProcessingChain<TimeType, SizeType>::is_ready() const {
    return m_instance != nullptr && !m_state_restore_active;
}

template <typename TimeType, typename SizeType>
bool CarlaLV2ProcessingChain<TimeType, SizeType>::is_active() const {
    return m_active;
}

template <typename TimeType, typename SizeType>
void CarlaLV2ProcessingChain<TimeType, SizeType>::set_active(bool active) {
    m_active = active;
}

template <typename TimeType, typename SizeType>
void CarlaLV2ProcessingChain<TimeType, SizeType>::process(uint32_t frames) {
    uint32_t processed = 0;

    while (processed < frames) {
        auto process = std::min(frames - processed,
                                carla_constants::max_buffer_size);
        if (!m_active || !m_instance) {
            return;
        }

        if (frames > m_internal_buffers_size) {
            throw_error<std::runtime_error>(
                "Carla processing chain: requesting to process more than buffer size ({} vs. {}).",
                frames, m_internal_buffers_size);
        }
        lilv_instance_activate(m_instance);
        lilv_instance_run(m_instance, process);
        lilv_instance_deactivate(m_instance);

        processed += process;
    }
}

template <typename TimeType, typename SizeType>
std::vector<typename CarlaLV2ProcessingChain<
    TimeType, SizeType>::SharedInternalAudioPort> const &
CarlaLV2ProcessingChain<TimeType, SizeType>::input_audio_ports() const {
    return m_input_audio_ports;
}

template <typename TimeType, typename SizeType>
std::vector<typename CarlaLV2ProcessingChain<
    TimeType, SizeType>::SharedInternalAudioPort> const &
CarlaLV2ProcessingChain<TimeType, SizeType>::output_audio_ports() const {
    return m_output_audio_ports;
}

template <typename TimeType, typename SizeType>
std::vector<typename CarlaLV2ProcessingChain<TimeType,
                                             SizeType>::SharedMidiPort> const &
CarlaLV2ProcessingChain<TimeType, SizeType>::input_midi_ports() const {
    return m_generic_input_midi_ports;
}

template <typename TimeType, typename SizeType>
bool CarlaLV2ProcessingChain<TimeType, SizeType>::is_freewheeling() const {
    return false;
}

template <typename TimeType, typename SizeType>
void CarlaLV2ProcessingChain<TimeType, SizeType>::set_freewheeling(
    bool enabled) {}

template <typename TimeType, typename SizeType>
CarlaLV2ProcessingChain<TimeType, SizeType>::~CarlaLV2ProcessingChain() {
    std::cout << "Destroying Carla processing chain." << std::endl;
    stop();
}

template <typename TimeType, typename SizeType>
void CarlaLV2ProcessingChain<TimeType, SizeType>::deserialize_state(
    std::string str) {
    while (!is_ready()) {
        std::this_thread::sleep_for(std::chrono::milliseconds(50));
    }
    if (!m_state_iface) {
        throw std::runtime_error("No state interface for Carla chain");
    }
    m_state_restore_active = true;
    std::thread restore_thread([this, str]() {
        try {
            LV2StateString s(
                this, CarlaLV2ProcessingChain<TimeType, SizeType>::map_urid,
                CarlaLV2ProcessingChain<TimeType, SizeType>::unmap_urid);
            static const LV2_Feature *features[] = {nullptr};
            s.deserialize(str);
            m_state_iface->restore(
                lilv_instance_get_handle(m_instance), LV2StateString::retrieve,
                (LV2_State_Handle)&s, LV2_STATE_IS_POD | LV2_STATE_IS_PORTABLE,
                features);
        } catch (const std::exception &exp) {
            log<log_level_error>("Failed to restore Carla state: {}", exp.what());
        } catch (...) {
            log<log_level_error>("Failed to restore Carla state: unknown exception");
        }

        m_state_restore_active = false;
    });
    restore_thread.detach();
}

template <typename TimeType, typename SizeType>
std::string CarlaLV2ProcessingChain<TimeType, SizeType>::serialize_state(uint32_t timeout_ms) {
    auto start = std::chrono::high_resolution_clock::now();
    auto timeout_expired = [start, timeout_ms]() {
        return std::chrono::high_resolution_clock::now() - start >
               std::chrono::milliseconds(timeout_ms);
    };
    while (!is_ready() && !timeout_expired()) {
        std::this_thread::sleep_for(std::chrono::milliseconds(50));
    }
    if (timeout_expired()) {
        log<log_level_error>("{}: Timeout waiting for Carla chain to be ready", m_human_name);
        throw std::runtime_error("Timeout waiting for Carla chain to be ready");
    }
    if (!m_state_iface) {
        log<log_level_error>("{}: No state interface for Carla chain", m_human_name);
        throw std::runtime_error("No state interface for Carla chain");
    }
    LV2StateString s(this,
                     CarlaLV2ProcessingChain<TimeType, SizeType>::map_urid,
                     CarlaLV2ProcessingChain<TimeType, SizeType>::unmap_urid);
    static const LV2_Feature *features[] = {nullptr};
    m_state_iface->save(lilv_instance_get_handle(m_instance),
                        LV2StateString::store, (LV2_State_Handle)&s,
                        LV2_STATE_IS_POD | LV2_STATE_IS_PORTABLE, features);
    return s.serialize();
}

template class CarlaLV2ProcessingChain<uint32_t, uint16_t>;
template class CarlaLV2ProcessingChain<uint32_t, uint32_t>;
template class CarlaLV2ProcessingChain<uint16_t, uint16_t>;
template class CarlaLV2ProcessingChain<uint16_t, uint32_t>;