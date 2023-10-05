#pragma once
#include <jack/types.h>
#include <jack_wrappers.h>

// Hide the JACK API behind a class with redirections, such that
// we can mock it out in tests.
// TODO: this does not contain the complete API, only what we use
class JackApi {
    static constexpr auto get_ports = &jack_get_ports;
    static constexpr auto port_by_name = &jack_port_by_name;
    static constexpr auto port_flags = &jack_port_flags;
    static constexpr auto port_type = &jack_port_type;
    static constexpr auto port_get_all_connections = &jack_port_get_all_connections;
    static constexpr auto port_get_buffer = &jack_port_get_buffer;
    static constexpr auto port_get_latency_range = &jack_port_get_latency_range;
    static constexpr auto port_get_latency = &jack_port_get_latency;
    static constexpr auto client_open = &jack_client_open;
    static constexpr auto get_client_name = &jack_get_client_name;
    static constexpr auto get_sample_rate = &jack_get_sample_rate;
    static constexpr auto get_buffer_size = &jack_get_buffer_size;
    static constexpr auto activate = &jack_activate;
    static constexpr auto client_close = &jack_client_close;
    static constexpr auto get_time = &jack_get_time;
    static constexpr auto set_process_callback = &jack_set_process_callback;
    static constexpr auto set_xrun_callback = &jack_set_xrun_callback;
    static constexpr auto set_port_connect_callback = &jack_set_port_connect_callback;
    static constexpr auto set_port_registration_callback = &jack_set_port_registration_callback;
    static constexpr auto set_port_rename_callback = &jack_set_port_rename_callback;
    static constexpr auto port_register = &jack_port_register;
    static constexpr auto port_unregister = &jack_port_unregister;
    static constexpr auto port_name = &jack_port_name;
    static constexpr auto connect = &jack_connect;
    static constexpr auto disconnect = &jack_disconnect;
    static constexpr auto midi_get_event_count = &jack_midi_get_event_count;
    static constexpr auto midi_event_get = &jack_midi_event_get;
    static constexpr auto midi_clear_buffer = &jack_midi_clear_buffer;
    static constexpr auto midi_event_write = &jack_midi_event_write;
};