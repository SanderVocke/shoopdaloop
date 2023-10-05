#pragma once
#include <cstring>
#include <jack/types.h>
#include <jack_wrappers.h>
#include <functional>
#include <string>
#include <map>
#include <tuple>
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
        Port(Port const& other) = delete;

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
        Client(Client const& other) = delete;

        void close() { valid = false; }
        void activate() { active = true; }
        void deactivate() { active = false; }
        Port &open_port(std::string name, Direction direction, Type type) {
            if (ports.find(name) != ports.end()) { return ports.at(name); }

            ports.try_emplace(name, name, *this, type, direction);
            return ports.at(name);
        }
        void close_port(std::string name) {}
    };

    static std::map<std::string, Client> ms_clients;
    static logging::logger *ms_logger;
    static JackPortRegistrationCallback ms_port_registration_callback;
    static void* ms_port_registration_callback_arg;

    static void* port_get_buffer(auto ...args) { ms_logger->trace("UNIMPL port_get_buffer"); return nullptr; };
    static void port_get_latency_range(auto ...args) { ms_logger->trace("UNIMPL port_get_latency_range"); return; };
    static jack_nframes_t port_get_latency(auto ...args) { ms_logger->trace("UNIMPL port_get_latency"); return 0; };
    static int set_process_callback(auto ...args) { ms_logger->trace("UNIMPL set_process_callback"); return 0; };
    static int set_xrun_callback(auto ...args) { ms_logger->trace("UNIMPL set_xrun_callback"); return 0; };
    static int set_port_connect_callback(auto ...args) { ms_logger->trace("UNIMPL set_port_connect_callback"); return 0; };
    static int set_port_rename_callback(auto ...args) { ms_logger->trace("UNIMPL set_port_rename_callback"); return 0; };
    static uint32_t midi_get_event_count(auto ...args) { ms_logger->trace("UNIMPL midi_get_event_count"); return 0; };
    static int midi_event_get(auto ...args) { ms_logger->trace("UNIMPL midi_event_get"); return 0; };
    static void midi_clear_buffer(auto ...args) { ms_logger->trace("UNIMPL midi_clear_buffer"); return; };
    static int midi_event_write(auto ...args) { ms_logger->trace("UNIMPL midi_event_write"); return 0; };
    static int port_unregister(auto ...args) { ms_logger->trace("UNIMPL port_unregister"); return 0; };
    static int connect(auto ...args) { ms_logger->trace("UNIMPL connect"); return 0; };
    static int disconnect(auto ...args) { ms_logger->trace("UNIMPL disconnect"); return 0; };
    static int activate(auto ...args) { ms_logger->trace("UNIMPL activate"); return 0; };
    static int client_close(auto ...args) { ms_logger->trace("UNIMPL client_close"); return 0; };
    static jack_nframes_t get_sample_rate(auto ...args) { ms_logger->trace("UNIMPL get_sample_rate"); return 48000; }
    static jack_nframes_t get_buffer_size(auto ...args) { ms_logger->trace("UNIMPL get_buffer_size"); return 2048; }

    static jack_client_t* client_open(const char* name, jack_options_t options, jack_status_t* status, ...);
    static void init();
    static const char** port_get_all_connections(const jack_client_t* client,const jack_port_t* port);
    static const char** port_get_connections(const jack_port_t* port);
    static int set_port_registration_callback(jack_client_t* client, JackPortRegistrationCallback cb, void* arg);

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