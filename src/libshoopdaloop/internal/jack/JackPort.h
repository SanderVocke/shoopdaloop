#pragma once
#include <cstdint>
#include <jack/types.h>
#include "JackTestApi.h"
#include "LoggingEnabled.h"
#include "PortInterface.h"
#include "JackAllPorts.h"
#include "JackApi.h"
#include <memory>

template<typename API>
class GenericJackPort :
    public virtual PortInterface,
    protected ModuleLoggingEnabled<"Backend.JackPort">
{
protected:
    jack_port_t* m_port;
    void* m_buffer;
    jack_client_t* m_client;
    std::string m_name;
    shoop_port_direction_t m_direction;
    PortDataType m_type;
    std::shared_ptr<GenericJackAllPorts<API>> m_all_ports_tracker;

public:
    const char* name() const override;
    void close() override;

    void* maybe_driver_handle() const override;

    bool has_internal_read_access() const override { return true; }
    bool has_internal_write_access() const override { return true; }
    bool has_implicit_input_source() const override { return true; }
    bool has_implicit_output_sink() const override { return true; }

    jack_port_t *get_jack_port() const;
    void *get_buffer() const;

    PortExternalConnectionStatus get_external_connection_status() const override;
    void connect_external(std::string name) override;
    void disconnect_external(std::string name) override;

    // Prepare step will get our JACK buffer.
    void PROC_prepare(uint32_t nframes) override;

    GenericJackPort(
        std::string name,
        shoop_port_direction_t direction,
        PortDataType type,
        jack_client_t *client,
        std::shared_ptr<GenericJackAllPorts<API>> all_ports_tracker
    );
    ~GenericJackPort() override;
};

using JackPort = GenericJackPort<JackApi>;
using JackTestPort = GenericJackPort<JackTestApi>;

extern template class GenericJackPort<JackApi>;
extern template class GenericJackPort<JackTestApi>;