#pragma once
#include <atomic>
#include <cstdint>
#include <jack/types.h>
#include <memory>
#include "JackAllPorts.h"
#include "JackApi.h"
#include "LoggingEnabled.h"
#include "PortInterface.h"

class JackPort :
    public virtual PortInterface,
    protected ModuleLoggingEnabled<"Backend.JackPort">
{
protected:
    std::shared_ptr<IJackApi> m_api;
    jack_port_t* m_port = nullptr;
    std::atomic<void*> m_buffer = nullptr;
    jack_client_t* m_client = nullptr;
    std::string m_name = "";
    shoop_port_direction_t m_direction = ShoopPortDirection_Input;
    PortDataType m_type;
    std::shared_ptr<JackAllPorts> m_all_ports_tracker;

public:
    const char* name() const override;
    void close() override;

    void* maybe_driver_handle() const override;

    bool has_internal_read_access() const override { return m_direction == ShoopPortDirection_Input; }
    bool has_internal_write_access() const override { return m_direction == ShoopPortDirection_Output; }
    bool has_implicit_input_source() const override { return m_direction == ShoopPortDirection_Input; }
    bool has_implicit_output_sink() const override { return m_direction == ShoopPortDirection_Output; }

    jack_port_t *get_jack_port() const;
    void *get_buffer() const;

    PortExternalConnectionStatus get_external_connection_status() const override;
    void connect_external(std::string name) override;
    void disconnect_external(std::string name) override;

    // Prepare step will get our JACK buffer.
    void PROC_prepare(uint32_t nframes) override;

    JackPort(
        std::string name,
        shoop_port_direction_t direction,
        PortDataType type,
        jack_client_t *client,
        std::shared_ptr<JackAllPorts> all_ports_tracker,
        std::shared_ptr<IJackApi> api
    );
    ~JackPort() override;
};
