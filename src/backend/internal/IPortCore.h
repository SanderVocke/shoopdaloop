#pragma once
#include <string>
#include <map>
#include "types.h"
#include <stdint.h>

// Forward declarations
class PortInterface;

/**
 * IPortCore - Pure virtual interface for core port functionality.
 * 
 * This interface defines the minimal contract that all ports must implement,
 * providing basic identification, lifecycle, and access control.
 * It is designed to be compatible with Rust trait-based composition.
 */
class IPortCore {
public:
    virtual ~IPortCore() = default;

    /// Returns the name of the port.
    virtual const char* name() const = 0;

    /// Closes the port and releases resources.
    virtual void close() = 0;

    /// Returns the data type of the port (Audio or MIDI).
    virtual PortDataType type() const = 0;

    /// Returns a driver-specific handle if available, nullptr otherwise.
    virtual void* maybe_driver_handle() const = 0;

    /// Returns true if other elements inside ShoopDaLoop can read from this port.
    virtual bool has_internal_read_access() const = 0;

    /// Returns true if other elements inside ShoopDaLoop can write to this port.
    virtual bool has_internal_write_access() const = 0;

    /// Returns true if the port gets data from an implicit external source.
    virtual bool has_implicit_input_source() const = 0;

    /// Returns true if the port sends data to an implicit external sink.
    virtual bool has_implicit_output_sink() const = 0;
};