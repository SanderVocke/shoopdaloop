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
        bool valid;

        Port(std::string name, Client &client, Type type, Direction direction) : name(name), type(type), direction(direction), valid(true) {
            connections.push_back(nullptr);
        }

        void* get_buffer() { return nullptr; }
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
    static logging::logger* ms_logger;

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

    static jack_client_t* client_open(const char* name, jack_options_t options, jack_status_t* status, ...) {
        ms_clients.emplace(name, Client(name));
        *status = (jack_status_t)0;
        auto rval = ms_clients.at(name).as_ptr();
        //ms_logger.debug("Create client {} -> {}", name, fmt::ptr(rval));
        return rval;
    }

    static void init() {
        //ms_logger.debug("Initializing");
        static bool initialized = false;
        if (initialized) { return; }

        ms_logger = &logging::get_logger("Backend.JackTestApi");

        // Populate the test environment with two clients, each having some ports.
        jack_status_t s;
        auto test_client_1 = client_open("test_client_1", JackNullOption, &s);
        auto test_client_2 = client_open("test_client_2", JackNullOption, &s);

        for (auto &c : {test_client_1, test_client_2}) {
            Client::from_ptr(c).open_port("audio_in", Direction::Input, Type::Audio);
            Client::from_ptr(c).open_port("audio_out", Direction::Output, Type::Audio);
            Client::from_ptr(c).open_port("midi_in", Direction::Input, Type::Midi);
            Client::from_ptr(c).open_port("midi_out", Direction::Output, Type::Midi);
        }

        initialized = true;
    }

    static const char** port_get_all_connections(const jack_client_t* client,const jack_port_t* port) {
        auto rval = port_get_connections(port);
        return rval;
    }

    static const char** port_get_connections(const jack_port_t* port) {
        auto p = Port::from_ptr((jack_port_t*) port);
        auto &conns = p.get_connections();
        auto rval = conns.data();
        //ms_logger.debug("Get all connections for port {} -> {}", fmt::ptr(port), fmt::ptr(rval));
        return rval;
    }

    static jack_port_t* port_register(
        jack_client_t* client,
        const char* port_name,
        const char* port_type,
        unsigned long flags,
        unsigned long bufsize)
    {
        auto c = Client::from_ptr(client);
        auto p = c.open_port(port_name, flags & JackPortIsInput ? Direction::Input : Direction::Output,
        std::string(port_type) == std::string(JACK_DEFAULT_AUDIO_TYPE) ? Type::Audio : Type::Midi);
        auto rval = p.as_ptr();
        //ms_logger.debug("Register port {} -> {}", port_name, fmt::ptr(rval));
        return rval;
    };

    static int port_unregister(auto ...args) { return 0; };
    static int connect(auto ...args) { return 0; };
    static int disconnect(auto ...args) { return 0; };
    static int activate(auto ...args) { return 0; };
    static int client_close(auto ...args) { return 0; };

    static const char** get_ports(jack_client_t * client,
		const char *  	port_name_pattern,
		const char *  	type_name_pattern,
		unsigned long  	flags)
    {
        // TODO implement regex patterns
        std::vector<Port*> ports;
        for(auto &c : ms_clients) {
            for(auto &p : c.second.ports) {
                ports.push_back(&p.second);
            }
        }

        auto it = std::remove_if(ports.begin(), ports.end(), [&](Port* p) {
            return (p->get_flags() & flags) != flags;
        });
        ports.erase(it, ports.end());

        auto c = Client::from_ptr(client);
        const char** port_names = new const char*[ports.size() + 1];
        for (size_t i=0; i<ports.size(); i++) {
            auto name = c.name + ":" + ports[i]->name;
            port_names[i] = strdup(name.c_str());
        }
        port_names[ports.size()] = nullptr;

        //ms_logger.debug("Get ports: {} ports found", ports.size());

        return port_names;
    };

    static jack_port_t* port_by_name(jack_client_t* client, const char *name) {
        std::string _name(name);
        size_t colon = _name.find(':');
        jack_port_t *rval = nullptr;
        if (colon != std::string::npos) {
            auto cname = _name.substr(0, colon);
            auto pname = _name.substr(colon + 1);

            if (ms_clients.find(cname) != ms_clients.end()) {
                auto &port_client = ms_clients.at(cname);
                if (port_client.ports.find(pname) != port_client.ports.end()) {
                    rval = port_client.ports.at(pname).as_ptr();
                }
            }
        }
        
        //ms_logger.debug("Get port by name {} -> {}", name, fmt::ptr(rval));
        return rval;
    }

    static int port_flags(const jack_port_t* port) {
        auto p = Port::from_ptr((jack_port_t*) port);
        switch (p.direction) {
            case Direction::Input:  return JackPortIsInput;
            case Direction::Output: return JackPortIsOutput;
        }
    }

    static const char* port_type(const jack_port_t* port) {
        auto p = Port::from_ptr((jack_port_t*) port);
        switch (p.type) {
            case Type::Audio: return JACK_DEFAULT_AUDIO_TYPE;
            case Type::Midi: return JACK_DEFAULT_MIDI_TYPE;
        }
    }

    static const char* port_name(const jack_port_t* port) {
        return Port::from_ptr((jack_port_t*) port).name.c_str();
    }

    static const char* get_client_name(jack_client_t *client) {
        return Client::from_ptr(client).name.c_str();
    }

    static jack_nframes_t get_sample_rate(auto ...args) { return 48000; }

    static jack_nframes_t get_buffer_size(auto ...args) { return 2048; }
};