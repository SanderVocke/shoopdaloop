#include "JackTestApi.h"
#include "LoggingBackend.h"
#include <jack/types.h>

namespace jacktestapi_globals {
    std::map<std::string, JackTestApi::Client> clients;
    JackPortRegistrationCallback port_registration_callback = nullptr;
    void *port_registration_callback_arg = nullptr;
}

jack_client_t* JackTestApi::client_open(const char* name, jack_options_t options, jack_status_t* status, ...) {
    jacktestapi_globals::clients.try_emplace(name, name);
    *status = (jack_status_t)0;
    jack_client_t* rval = jacktestapi_globals::clients.at(name).as_ptr();
    logging::log<"Backend.JackTestApi", trace>(std::nullopt, std::nullopt, "Create client {} -> {}", name, (void*)rval);
    return rval;
}

void JackTestApi::init() {
    static bool initialized = false;
    if (initialized) { return; }

    logging::log<"Backend.JackTestApi", trace>(std::nullopt, std::nullopt, "Initializing JackTestApi");

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

const char** JackTestApi::port_get_all_connections(const jack_client_t* client,const jack_port_t* port) {
    auto rval = port_get_connections(port);
    return rval;
}

const char** JackTestApi::port_get_connections(const jack_port_t* port) {
    auto &p = Port::from_ptr((jack_port_t*) port);
    auto &conns = p.get_connections();
    auto rval = new const char*[conns.size() + 1];
    uint32_t i=0;
    for (auto it = conns.begin(); it != conns.end(); it++, i++) {
        rval[i] = strdup(it->c_str());
    }
    rval[conns.size()] = nullptr;
    logging::log<"Backend.JackTestApi", trace>(std::nullopt, std::nullopt, "Get all connections for port {} -> {} connections", (void*)port, conns.size());
    return rval;
}

jack_port_t* JackTestApi::port_register(
    jack_client_t* client,
    const char* port_name,
    const char* port_type,
    unsigned long flags,
    unsigned long bufsize)
{
    auto &c = Client::from_ptr(client);
    auto &p = c.open_port(port_name, flags & JackPortIsInput ? Direction::Input : Direction::Output,
    std::string(port_type) == std::string(JACK_DEFAULT_AUDIO_TYPE) ? Type::Audio : Type::Midi);
    
    if (jacktestapi_globals::port_registration_callback) {
        jacktestapi_globals::port_registration_callback(-1, 1, jacktestapi_globals::port_registration_callback_arg);
    }
    
    auto rval = p.as_ptr();
    logging::log<"Backend.JackTestApi", trace>(std::nullopt, std::nullopt, "Register port {} -> {}", port_name, (void*)rval);
    return rval;
};

const char** JackTestApi::get_ports(jack_client_t * client,
    const char *  	port_name_pattern,
    const char *  	type_name_pattern,
    unsigned long  	flags)
{
    // TODO implement regex patterns
    std::vector<Port*> ports;
    for(auto &c : jacktestapi_globals::clients) {
        for(auto &p : c.second.ports) {
            ports.push_back(&p.second);
        }
    }

    auto it = std::remove_if(ports.begin(), ports.end(), [&](Port* p) {
        return (p->get_flags() & flags) != flags;
    });
    ports.erase(it, ports.end());

    const char** port_names = new const char*[ports.size() + 1];
    for (uint32_t i=0; i<ports.size(); i++) {
        auto &c = ports[i]->client;
        auto name = c.name + ":" + ports[i]->name;
        port_names[i] = strdup(name.c_str());
        logging::log<"Backend.JackTestApi", trace>(std::nullopt, std::nullopt, "Get ports: {} -> {}", i, port_names[i]);
    }
    port_names[ports.size()] = nullptr;

    auto nports = ports.size();
    logging::log<"Backend.JackTestApi", trace>(std::nullopt, std::nullopt, "Get ports: {} ports found", nports);

    return port_names;
};

jack_port_t* JackTestApi::port_by_name(jack_client_t* client, const char *name) {
    std::string _name(name);
    size_t colon = _name.find(':');
    jack_port_t *rval = nullptr;
    std::string cname, pname;
    if (colon != std::string::npos) {
        cname = _name.substr(0, colon);
        pname = _name.substr(colon + 1);
    } else {
        cname = Client::from_ptr(client).name;
        pname = _name;
    }

    logging::log<"Backend.JackTestApi", debug>(std::nullopt, std::nullopt, "name parts: {} : {}", cname, pname);

    if (jacktestapi_globals::clients.find(cname) != jacktestapi_globals::clients.end()) {
        auto &port_client = jacktestapi_globals::clients.at(cname);
        if (port_client.ports.find(pname) != port_client.ports.end()) {
            rval = port_client.ports.at(pname).as_ptr();
        }
    }
    
    logging::log<"Backend.JackTestApi", trace>(std::nullopt, std::nullopt, "Get port by name {} -> {}", name, (void*)rval);
    return rval;
}

int JackTestApi::port_flags(const jack_port_t* port) {
    auto &p = Port::from_ptr((jack_port_t*) port);
    int rval;
    switch (p.direction) {
        case Direction::Input:  rval = JackPortIsInput; break;
        case Direction::Output: rval = JackPortIsOutput; break;
    }

    logging::log<"Backend.JackTestApi", trace>(std::nullopt, std::nullopt, "Get port flags {} -> {}", (void*)port, rval);
    return rval;
}

const char* JackTestApi::port_type(const jack_port_t* port) {
    auto &p = Port::from_ptr((jack_port_t*) port);
    const char* rval;
    switch (p.type) {
        case Type::Audio: rval = JACK_DEFAULT_AUDIO_TYPE; break;
        case Type::Midi: rval = JACK_DEFAULT_MIDI_TYPE; break;
    }

    logging::log<"Backend.JackTestApi", trace>(std::nullopt, std::nullopt, "Get port type {} -> {}", (void*)port, rval);
    return rval;
}

const char* JackTestApi::port_name(const jack_port_t* port) {
    auto rval = strdup(Port::from_ptr((jack_port_t*) port).name.c_str());
    logging::log<"Backend.JackTestApi", trace>(std::nullopt, std::nullopt, "Get port name {} -> {}", (void*)port, rval);
    return rval;
}

const char* JackTestApi::get_client_name(jack_client_t *client) {
    auto rval = strdup(Client::from_ptr(client).name.c_str());
    logging::log<"Backend.JackTestApi", trace>(std::nullopt, std::nullopt, "Get client name {} -> {}", (void*)client, rval);
    return rval;
}

int JackTestApi::set_port_registration_callback(jack_client_t* client, JackPortRegistrationCallback cb, void* arg) {
    logging::log<"Backend.JackTestApi", trace>(std::nullopt, std::nullopt, "Set port registration cb for client {}, arg {}", (void*)client, arg);
    jacktestapi_globals::port_registration_callback = cb;
    jacktestapi_globals::port_registration_callback_arg = arg;
    return 0;
}

void JackTestApi::set_error_function(void (*fn)(const char*)) {}
void JackTestApi::set_info_function(void (*fn)(const char*)) {}

int JackTestApi::connect(jack_client_t* client, const char* src, const char* dst) {
    auto &_src = Port::from_ptr(port_by_name(client, src));
    auto &_dst = Port::from_ptr(port_by_name(client, dst));

    _src.connections.insert(dst);
    _dst.connections.insert(src);

    logging::log<"Backend.JackTestApi", trace>(std::nullopt, std::nullopt, "Connect {} {}", _src.name, _dst.name);
    
    return 0;
};

int JackTestApi::disconnect(jack_client_t* client, const char* src, const char* dst) {
    auto &_src = Port::from_ptr(port_by_name(client, src));
    auto &_dst = Port::from_ptr(port_by_name(client, dst));

    _src.connections.erase(dst);
    _dst.connections.erase(src);

    logging::log<"Backend.JackTestApi", trace>(std::nullopt, std::nullopt, "Disconnect {} {}", _src.name, _dst.name);
    
    return 0;
};