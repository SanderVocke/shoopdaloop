#pragma once
#include <jack/types.h>
#include <jack_wrappers.h>
#include <type_traits>
#include <stdarg.h>
#include <stdexcept>
#include <iostream>

// Hide the JACK API behind a class with redirections, such that
// we can mock it out in tests.
// TODO: this does not contain the complete API, only what we use
class JackApi {
public:
    static auto get_ports(auto ...args) { return jack_get_ports(args...); }
    static auto port_by_name(auto ...args) { return jack_port_by_name(args...); }
    static auto port_flags(auto ...args) { return jack_port_flags(args...); }
    static auto port_type(auto ...args) { return jack_port_type(args...); }
    static auto port_get_all_connections(auto ...args) { return jack_port_get_all_connections(args...); }
    static auto port_get_buffer(auto ...args) { return jack_port_get_buffer(args...); }
    static auto port_get_latency_range(auto ...args) { return jack_port_get_latency_range(args...); }
    static auto port_get_latency(auto ...args) { return jack_port_get_latency(args...); }
    static auto get_client_name(auto ...args) { return jack_get_client_name(args...); }
    static auto get_sample_rate(auto ...args) { return jack_get_sample_rate(args...); }
    static auto get_buffer_size(auto ...args) { return jack_get_buffer_size(args...); }
    static auto activate(auto ...args) { return jack_activate(args...); }
    static auto client_close(auto ...args) { return jack_client_close(args...); }
    static auto get_time(auto ...args) { return jack_get_time(args...); }
    static auto set_process_callback(auto ...args) { return jack_set_process_callback(args...); }
    static auto set_xrun_callback(auto ...args) { return jack_set_xrun_callback(args...); }
    static auto set_port_connect_callback(auto ...args) { return jack_set_port_connect_callback(args...); }
    static auto set_port_registration_callback(auto ...args) { return jack_set_port_registration_callback(args...); }
    static auto set_port_rename_callback(auto ...args) { return jack_set_port_rename_callback(args...); }
    static auto port_register(auto ...args) { return jack_port_register(args...); }
    static auto port_unregister(auto ...args) { return jack_port_unregister(args...); }
    static auto port_name(auto ...args) { return jack_port_name(args...); }
    static auto connect(auto ...args) { return jack_connect(args...); }
    static auto disconnect(auto ...args) { return jack_disconnect(args...); }
    static auto midi_get_event_count(auto ...args) { return jack_midi_get_event_count(args...); }
    static auto midi_event_get(auto ...args) { return jack_midi_event_get(args...); }
    static auto midi_clear_buffer(auto ...args) { return jack_midi_clear_buffer(args...); }
    static auto midi_event_write(auto ...args) { return jack_midi_event_write(args...); }
    static auto cpu_load(auto ...args) { return jack_cpu_load(args...); }

    static jack_client_t* client_open(const char* name, jack_options_t options, jack_status_t* status, ...) {
        va_list args;
        va_start(args, status);
        auto rval = jack_client_open(name, options, status, args);
        va_end(args);
        return rval;
    }

    static void init() {
        static bool initialized = false;
        if (!initialized) {
            // We use a wrapper which dlopens Jack to not have a hard linkage
            // dependency. It needs to be initialized first.
            if (initialize_jack_wrappers(0)) {
                throw std::runtime_error("Unable to find Jack client library.");
            }
            initialized = true;
        }
    }
};