#pragma once
#include "InternalLV2MidiOutputPort.h"
#include "ProcessingChainInterface.h"
#include "LoggingEnabled.h"
#include "ProcessProfiling.h"
#include "ExternalUIInterface.h"
#include "SerializeableStateInterface.h"
#include <string>
#include <vector>
#include <map>
#include <types.h>
#include <lilv/lilv.h>
#include <lv2/core/lv2.h>
#include <lv2/urid/urid.h>
#include <lv2/options/options.h>
#include <lv2/buf-size/buf-size.h>
#include <lv2/ui/ui.h>
#include <lv2/instance-access/instance-access.h>
#include <lv2_external_ui.h>
#include <lv2_evbuf.h>
#include <lv2/midi/midi.h>
#include <lv2/atom/atom.h>
#include <lv2/state/state.h>
#include <lv2/event/event.h>
#include <lv2/event/event-helpers.h>

class LV2StateString {
    // map of key => (type, value)
    std::map<std::string, std::pair<std::string, std::vector<uint8_t>>> data;
    void * chain;
    LV2_URID (*map_urid)(LV2_URID_Map_Handle handle, const char* uri);
    const char* (*unmap_urid)(LV2_URID_Unmap_Handle handle, LV2_URID urid);

    LV2_URID do_map_urid(const char* uri);

    const char* do_unmap_urid(LV2_URID urid);
    
public:
    LV2StateString(
        decltype(chain) _chain,
        decltype(map_urid) _map_urid,
        decltype(unmap_urid) _unmap_urid);

    static LV2_State_Status store(LV2_State_Handle handle,
                                    uint32_t key,
                                    const void *value,
                                    size_t size,
                                    uint32_t type,
                                    uint32_t flags);

    LV2_State_Status _store(std::string key_uri,
                        const void* value,
                        size_t size,
                        std::string type_uri,
                        uint32_t flags);

    static const void* retrieve(LV2_State_Handle handle,
                                        uint32_t key,
                                        size_t *size,
                                        uint32_t *type,
                                        uint32_t *flags);

    const void *_retrieve(std::string key_uri,
                        size_t &size_out,
                        std::string &type_uri_out,
                        uint32_t &flags_out);

    std::string serialize() const;
    void deserialize(std::string str);
};

template<typename TimeType, typename SizeType>
class CarlaLV2ProcessingChain : public ProcessingChainInterface<TimeType, SizeType>,
                                public ModuleLoggingEnabled,
                                public ExternalUIInterface,
                                public SerializeableStateInterface {
public:
    using _InternalAudioPort = typename ProcessingChainInterface<TimeType, SizeType>::_InternalAudioPort;
    using SharedInternalAudioPort = typename ProcessingChainInterface<TimeType, SizeType>::SharedInternalAudioPort;
    using MidiOutputPort = InternalLV2MidiOutputPort;
    using SharedMidiOutputPort = std::shared_ptr<MidiOutputPort>;
    using MidiPort = typename ProcessingChainInterface<TimeType, SizeType>::MidiPort;
    using SharedMidiPort = typename ProcessingChainInterface<TimeType, SizeType>::SharedMidiPort;

    std::string log_module_name() const override;

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
    LV2_URID m_midi_event_type, m_atom_chunk_type, m_atom_sequence_type, m_atom_int_type, m_maxbuffersize_type, m_minbuffersize_type;
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
    std::shared_ptr<profiling::ProfilingItem> m_maybe_profiling_item = nullptr;

    const LV2UI_Descriptor * m_ui_descriptor = nullptr;

    std::vector<SharedInternalAudioPort> m_input_audio_ports;
    std::vector<SharedInternalAudioPort> m_output_audio_ports;
    std::vector<SharedMidiOutputPort> m_input_midi_ports;
    std::vector<SharedMidiPort> m_generic_input_midi_ports;
    std::vector<LV2_Evbuf*> m_output_midi_ports; // Note that these will not be used. Only there to satisfy the plugin.
    
    using UridMap = std::map<const char*, LV2_URID>;
    UridMap m_urid_map;

    static LV2_URID map_urid(LV2_URID_Map_Handle handle, const char* uri);
    LV2_URID map_urid(const char* uri);
    static const char* unmap_urid(LV2_URID_Unmap_Handle handle, LV2_URID urid);
    const char* unmap_urid(LV2_URID urid);

    static void static_ui_write_fn(LV2UI_Controller controller,
                                   uint32_t port_index,
                                   uint32_t buffer_size,
                                   uint32_t port_protocol,
                                   const void* buffer);

    void ui_write_fn(uint32_t port_index, uint32_t buffer_size, uint32_t port_protocol, const void* buffer);
    static void static_ui_closed_fn(LV2UI_Controller controller);
    void reconnect_ports();
    void ui_closed_fn();
    void maybe_cleanup_ui();

public:
    CarlaLV2ProcessingChain(
        LilvWorld *lilv_world,
        fx_chain_type_t type,
        size_t sample_rate,
        std::string human_name,
        std::shared_ptr<profiling::Profiler> maybe_profiler = nullptr
    );

    void instantiate(size_t sample_rate);
    bool visible() const override;
    void show() override;
    void hide() override;
    void stop() override;
    bool is_ready() const override;
    bool is_active() const override;
    void set_active(bool active) override;
    void process(size_t frames) override;

    std::vector<SharedInternalAudioPort> const& input_audio_ports() const override;
    std::vector<SharedInternalAudioPort> const& output_audio_ports() const override;
    std::vector<SharedMidiPort> const& input_midi_ports() const override;
    bool is_freewheeling() const override;
    void set_freewheeling(bool enabled) override;

    void ensure_buffers(size_t size) override;
    size_t buffers_size() const override;

    virtual ~CarlaLV2ProcessingChain();

    void deserialize_state(std::string str) override;
    std::string serialize_state() override;
};

extern template class CarlaLV2ProcessingChain<uint32_t, uint16_t>;
extern template class CarlaLV2ProcessingChain<uint32_t, uint32_t>;
extern template class CarlaLV2ProcessingChain<uint16_t, uint16_t>;
extern template class CarlaLV2ProcessingChain<uint16_t, uint32_t>;