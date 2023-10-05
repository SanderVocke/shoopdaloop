#pragma once
#include <jack/types.h>
#include <jack_wrappers.h>
#include <functional>
#include <string>
#include <map>

// A partial simple mock implementation of the JACK API which can be used for simple
// tests of connectivity (creating and manipulating clients, ports).
// Note that only creation is supported. Closing clients and ports will not do anything.
class JackTestApi {
public:
    enum class Type {
        Audio,
        Midi
    };

    enum class Direction {
        Input,
        Output
    };

    struct Port;
    struct Client;

    struct Port {
        jack_port_t *as_ptr() { return reinterpret_cast<jack_port_t*>(this); }
        static Port &from_ptr(jack_port_t *ptr) { return *reinterpret_cast<Port*>(ptr); }

        std::string name;
        Type type;
        Direction direction;
        std::vector<Port*> connections;
        bool valid;

        Port(std::string name, Client &client, Type type, Direction direction) : name(name), type(type), direction(direction), valid(true) {}

        void* get_buffer() { return nullptr; }
    };

    struct Client {
        jack_client_t *as_ptr() { return reinterpret_cast<jack_client_t*>(this); }
        static Client &from_ptr(jack_client_t *ptr) { return *reinterpret_cast<Client*>(ptr); }

        std::string name;
        bool active;
        bool valid;
        std::vector<Port> ports;

        Client(std::string name) : name(name), active(false), valid(true) {}

        void close() { valid = false; }
        void activate() { active = true; }
        void deactivate() { active = false; }
    };

    static std::map<std::string, Client> ms_clients;

    static const char** get_ports(auto ...args) { return nullptr; };
    static jack_port_t* port_by_name(auto ...args) { return nullptr; };
    static int port_flags(auto ...args) { return 0; };
    static const char* port_type(auto ...args) { return nullptr; };
    static const char** port_get_all_connections(auto ...args) { return nullptr; };
    static void* port_get_buffer(auto ...args) { return nullptr; };
    static void port_get_latency_range(auto ...args) { return; };
    static jack_nframes_t port_get_latency(auto ...args) { return 0; };
    static const char* get_client_name(auto ...args) { return nullptr; };
    static jack_nframes_t get_sample_rate(auto ...args) { return 0; };
    static jack_nframes_t get_buffer_size(auto ...args) { return 0; };
    static int activate(auto ...args) { return 0; };
    static int client_close(auto ...args) { return 0; };
    static int set_process_callback(auto ...args) { return 0; };
    static int set_xrun_callback(auto ...args) { return 0; };
    static int set_port_connect_callback(auto ...args) { return 0; };
    static int set_port_registration_callback(auto ...args) { return 0; };
    static int set_port_rename_callback(auto ...args) { return 0; };
    static jack_port_t* port_register(auto ...args) { return nullptr; };
    static int port_unregister(auto ...args) { return 0; };
    static const char* port_name(auto ...args) { return nullptr; };
    static int connect(auto ...args) { return 0; };
    static int disconnect(auto ...args) { return 0; };
    static uint32_t midi_get_event_count(auto ...args) { return 0; };
    static int midi_event_get(auto ...args) { return 0; };
    static void midi_clear_buffer(auto ...args) { return; };
    static int midi_event_write(auto ...args) { return 0; };

    static jack_client_t* client_open(const char* name, jack_options_t options, jack_status_t* status, ...) {
        ms_clients.emplace(name, Client(name));
        *status = (jack_status_t)0;
        return ms_clients.at(name).as_ptr();
    }

    static void init() {}
};