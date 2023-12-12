#pragma once
#include <string>
#include <map>
#include "types.h"
#include <stdint.h>

enum class PortDataType {
    Audio,
    Midi
};

// Used for abstractly connecting a port to external entities.
// The status includes the possible entities to connect to and
// whether the port is currently connected.
using PortExternalConnectionStatus = std::map<std::string, bool>;

// A port in ShoopDaLoop is a way to move data around. Ports may have internal
// buffering or processing.
// A port may get data from outside the app, inside the app or merged from multiple
// sources.
// A port should always generally be used as follows in chronological order:
// 1. Prepare the port. (e.g. create/get buffers, pre-process external data...)
// 2. Write into the port if applicable.
// 3. Process the port if applicable (e.g. sort MIDI messages, apply gain, ...)
// 4. Read out of the port if applicable.
class PortInterface {
public:
    virtual void close() = 0;
    virtual const char* name() const = 0;
    virtual PortDataType type() const = 0;
    virtual void* maybe_driver_handle() const = 0;

    // "internal" here refers to whether other elements inside
    // the ShoopDaLoop backend can read/write from/to the port.
    virtual bool has_internal_read_access() const = 0;
    virtual bool has_internal_write_access() const = 0;

    // "implicit" here means that the port gets data from somewhere
    // other than data written/read by other elements (e.g. the audio
    // driver or a special internal function).
    virtual bool has_implicit_input_source() const = 0;
    virtual bool has_implicit_output_sink() const = 0;

    // See above: any preparation that should happen at the start of the process
    // cycle, regardless of what other ports are doing.
    virtual void PROC_prepare(uint32_t nframes) = 0;

    // See above: any processing that should happen after input was received,
    // before output can be provided.
    virtual void PROC_process(uint32_t nframes) = 0;

    // Manage external connection. Note that the assumption here is that only either
    // input or output can be externally connected, not both.
    virtual PortExternalConnectionStatus get_external_connection_status() const = 0;
    virtual void connect_external(std::string name) = 0;
    virtual void disconnect_external(std::string name) = 0;

    virtual bool get_muted() const = 0;
    virtual void set_muted(bool muted) = 0;

    PortInterface() {}
    virtual ~PortInterface() {};
};