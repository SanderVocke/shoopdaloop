#pragma once
#include <jack/types.h>
#include "PortInterface.h"
#include "JackAllPorts.h"
#include <jack_wrappers.h>
#include <memory>

class JackPort :
    public PortInterface
{
protected:
    jack_port_t* m_port;
    jack_client_t* m_client;
    std::string m_name;
    PortDirection m_direction;
    PortType m_type;
    std::shared_ptr<JackAllPorts> m_all_ports_tracker;

public:
    const char* name() const override;
    PortDirection direction() const override;
    virtual PortType type() const override;
    void close() override;
    jack_port_t *get_jack_port() const;

    PortExternalConnectionStatus get_external_connection_status() const override;
    void connect_external(std::string name) override;
    void disconnect_external(std::string name) override;

    JackPort(
        std::string name,
        PortDirection direction,
        PortType type,
        jack_client_t *client,
        std::shared_ptr<JackAllPorts> all_ports_tracker
    );
    ~JackPort() override;
};