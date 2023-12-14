#pragma once
#include <jack/types.h>
#include <jack_wrappers.h>
#include <string>
#include <map>
#include <set>
#include "LoggingBackend.h"
#include "MidiMessage.h"
#include <vector>

namespace jacktestapi_globals {
    extern JackPortRegistrationCallback port_registration_callback;
    extern void* port_registration_callback_arg;
    extern std::map<void*, jack_port_t*> buffers_to_ports;
}

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
        std::set<std::string> connections;
        Client &client;
        bool valid;
        std::vector<float> audio_buffer;
        std::vector<MidiMessage<uint32_t, uint32_t>> midi_buffer;

        Port(std::string name, Client &client, Type type, Direction direction) : name(name), type(type), direction(direction), valid(true), client(client) {}
        Port(Port const& other) = delete;

        void* get_buffer(uint32_t nframes) {
            if (type == Type::Audio) {
                audio_buffer.resize(std::max((size_t)nframes, audio_buffer.size()));
                return (void*)audio_buffer.data();
            } else {
                return (void*)this;
            }
        }
        Client &get_client() { return client; }
        unsigned long get_flags() { return direction == Direction::Input ? JackPortIsInput : JackPortIsOutput; }
        std::set<std::string> &get_connections() { return connections; }
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

    static Port &internal_port_data(jack_port_t* port) {
        return Port::from_ptr(port);
    }

    static void* port_get_buffer(jack_port_t* port, jack_nframes_t nframes) {
        logging::log<"Backend.JackTestApi", log_level_trace>(std::nullopt, std::nullopt, "UNIMPL port_get_buffer");
        auto rval = Port::from_ptr(port).get_buffer(nframes);
        jacktestapi_globals::buffers_to_ports[rval] = port;
        return rval;
    };

    static void port_get_latency_range(auto ...args) {
        logging::log<"Backend.JackTestApi", log_level_trace>(std::nullopt, std::nullopt, "UNIMPL port_get_latency_range");
        return;
    };

    static jack_nframes_t port_get_latency(auto ...args) {
        logging::log<"Backend.JackTestApi", log_level_trace>(std::nullopt, std::nullopt, "UNIMPL port_get_latency");
        return 0;
    };

    static int set_process_callback(auto ...args) {
        logging::log<"Backend.JackTestApi", log_level_trace>(std::nullopt, std::nullopt, "UNIMPL set_process_callback");
        return 0;
    };

    static int set_xrun_callback(auto ...args) {
        logging::log<"Backend.JackTestApi", log_level_trace>(std::nullopt, std::nullopt, "UNIMPL set_xrun_callback");
        return 0;
    };
    
    static int set_port_connect_callback(auto ...args) {
        logging::log<"Backend.JackTestApi", log_level_trace>(std::nullopt, std::nullopt, "UNIMPL set_port_connect_callback");
        return 0;
    };

    static int set_port_rename_callback(auto ...args) {
        logging::log<"Backend.JackTestApi", log_level_trace>(std::nullopt, std::nullopt, "UNIMPL set_port_rename_callback");
        return 0;
    };

    static uint32_t midi_get_event_count(void* buffer) {
        auto &port = Port::from_ptr(jacktestapi_globals::buffers_to_ports[buffer]);
        return port.midi_buffer.size();
    };

    static int midi_event_get(jack_midi_event_t *event, void *port_buffer, uint32_t event_index) {
        auto &port = Port::from_ptr(jacktestapi_globals::buffers_to_ports[port_buffer]);
        auto &msg = port.midi_buffer[event_index];
        event->time = msg.time;
        event->size = msg.size;
        event->buffer = (jack_midi_data_t*) msg.data.data();
        return 0;
    };

    static void midi_clear_buffer(void *buffer) {
        auto &port = Port::from_ptr(jacktestapi_globals::buffers_to_ports[buffer]);
        port.midi_buffer.clear();
    };

    static int midi_event_write(void *port_buffer, jack_nframes_t time, const jack_midi_data_t *data, size_t data_size) {
        auto &port = Port::from_ptr(jacktestapi_globals::buffers_to_ports[port_buffer]);
        MidiMessage<uint32_t, uint32_t> msg;
        msg.data.resize(data_size);
        memcpy((void*) msg.data.data(), (void*) data, data_size);
        msg.time = time;
        msg.size = data_size;
        port.midi_buffer.push_back(msg);
        return 0;
    };

    static int port_unregister(auto ...args) { return 0; };

    static int activate(auto ...args) {
        logging::log<"Backend.JackTestApi", log_level_trace>(std::nullopt, std::nullopt, "UNIMPL activate");
        return 0;
    };

    static int client_close(auto ...args) { return 0; };

    static jack_nframes_t get_sample_rate(auto ...args) {
        logging::log<"Backend.JackTestApi", log_level_trace>(std::nullopt, std::nullopt, "UNIMPL get_sample_rate");
        return 48000;
    }

    static jack_nframes_t get_buffer_size(auto ...args) {
        logging::log<"Backend.JackTestApi", log_level_trace>(std::nullopt, std::nullopt, "UNIMPL get_buffer_size");
        return 2048;
    }
    
    static float cpu_load(auto ...args) {
        logging::log<"Backend.JackTestApi", log_level_trace>(std::nullopt, std::nullopt, "UNIMPL cpu_load");
        return 0.0f;
    }

    static jack_client_t* client_open(const char* name, jack_options_t options, jack_status_t* status, ...);
    static void init();
    static const char** port_get_all_connections(const jack_client_t* client,const jack_port_t* port);
    static const char** port_get_connections(const jack_port_t* port);
    static int set_port_registration_callback(jack_client_t* client, JackPortRegistrationCallback cb, void* arg);

    static void set_error_function(void (*fn)(const char*));
    static void set_info_function(void (*fn)(const char*));

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

    static int connect(jack_client_t* client, const char* src, const char* dst);
    static int disconnect(jack_client_t* client, const char* src, const char* dst);
};

namespace jacktestapi_globals {
    extern std::map<std::string, JackTestApi::Client> clients;
}