#pragma once
#include <cstring>
#include <jack/types.h>
#include <jack_wrappers.h>
#include <functional>
#include <string>
#include <map>
#include "LoggingBackend.h"

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
        std::vector<const char*> connections;
        Client &client;
        bool valid;

        Port(std::string name, Client &client, Type type, Direction direction) : name(name), type(type), direction(direction), valid(true), client(client) {
            connections.push_back(nullptr);
        }

        void* get_buffer() { return nullptr; }
        Client &get_client() { return client; }
        unsigned long get_flags() { return direction == Direction::Input ? JackPortIsInput : JackPortIsOutput; }
        std::vector<const char*> &get_connections() { return connections; }
    };

    struct Client {
        jack_client_t *as_ptr() { return reinterpret_cast<jack_client_t*>(this); }
        static Client &from_ptr(jack_client_t *ptr) { return *reinterpret_cast<Client*>(ptr); }

        std::string name;
        bool active;
        bool valid;
        std::map<std::string, Port> ports;

        Client(std::string name) : name(name), active(false), valid(true) {}

        void close() { valid = false; }
        void activate() { active = true; }
        void deactivate() { active = false; }
        Port &open_port(std::string name, Direction direction, Type type) {
            if (ports.find(name) != ports.end()) { return ports.at(name); }

            ports.emplace(name, Port(name, *this, type, direction));
            return ports.at(name);
        }
        void close_port(std::string name) {}
    };

    static std::map<std::string, Client> ms_clients;
    static logging::logger *ms_logger;

    static void* port_get_buffer(auto ...args) { return nullptr; };
    static void port_get_latency_range(auto ...args) { return; };
    static jack_nframes_t port_get_latency(auto ...args) { return 0; };
    static int set_process_callback(auto ...args) { return 0; };
    static int set_xrun_callback(auto ...args) { return 0; };
    static int set_port_connect_callback(auto ...args) { return 0; };
    static int set_port_registration_callback(auto ...args) { return 0; };
    static int set_port_rename_callback(auto ...args) { return 0; };
    static uint32_t midi_get_event_count(auto ...args) { return 0; };
    static int midi_event_get(auto ...args) { return 0; };
    static void midi_clear_buffer(auto ...args) { return; };
    static int midi_event_write(auto ...args) { return 0; };
    static int port_unregister(auto ...args) { return 0; };
    static int connect(auto ...args) { return 0; };
    static int disconnect(auto ...args) { return 0; };
    static int activate(auto ...args) { return 0; };
    static int client_close(auto ...args) { return 0; };
    static jack_nframes_t get_sample_rate(auto ...args) { return 48000; }
    static jack_nframes_t get_buffer_size(auto ...args) { return 2048; }

    static jack_client_t* client_open(const char* name, jack_options_t options, jack_status_t* status, ...);
    static void init();
    static const char** port_get_all_connections(const jack_client_t* client,const jack_port_t* port);
    static const char** port_get_connections(const jack_port_t* port);

    static jack_port_t* port_register(
        jack_client_t* client,
        const char* port_name,
        const char* port_type,
        unsigned long flags,
        unsigned long bufsize);

    static const char** get_ports(jack_client_t * client,
		const char *  	port_name_pattern,
		const char *  	type_name_pattern,
		unsigned long  	flags);

    static jack_port_t* port_by_name(jack_client_t* client, const char *name);
    static int port_flags(const jack_port_t* port);

    static const char* port_type(const jack_port_t* port);

    static const char* port_name(const jack_port_t* port);
    static const char* get_client_name(jack_client_t *client);

    
};