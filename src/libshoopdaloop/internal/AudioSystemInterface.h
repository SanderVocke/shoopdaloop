#pragma once
#include <optional>
#include <memory>
#include <stdio.h>
#include "PortInterface.h"
#include "MidiPortInterface.h"
#include <string>
#include "AudioPortInterface.h"
#include <functional>
#include <stdint.h>
#include "types.h"
class AudioSystemInterface {
public:

    AudioSystemInterface(
        std::string client_name,
        std::function<void(uint32_t)> process_cb
    ) {}


    /*
        On flexible port-based audio systems (e.g. JACK), these ports
        refer directly to such ports.
        On systems limited to individual devices, these calls will create virtual
        ports, of which the only way to get audio to/from the outside world is to
        route them to system-provided ports.
    */
    virtual
    std::shared_ptr<AudioPortInterface<audio_sample_t>> open_audio_port(
        std::string name,
        PortDirection direction
    ) = 0;
    virtual
    std::shared_ptr<MidiPortInterface> open_midi_port(
        std::string name,
        PortDirection direction
    ) = 0;

    /*
        System-provided ports can fulfill different roles depending on the
        back-end:
        - On JACK, there are none by default. They can be dynamically created if required.
          all individual ports are already connected to the outside world. See additional
          info below at create_system_..._port.
        - RtAudio-based back-end will provide system ports which are routed
          to the configured audio device.
    */
    virtual std::vector<std::shared_ptr<AudioPortInterface>> get_system_audio_ports() const = 0;
    virtual std::vector<std::shared_ptr<MidiPortInterface>> get_system_midi_ports() const = 0;

    /*
        As mentioned above, on e.g. JACK a system port can be created.
        E.g: creating a system audio input port will make the new port available to JACK on the outside,
        and available for connecting internally in the application too. Such ports can be used e.g. for
        mixing multiple other ports' data together.
        Note that the port direction for internal ports are defined w.r.t. ShoopDaLoop,
        looking "from the outside". So creating a system audio port with direction "out" will make it
        available as an output externally and as a data sink internally.
    */
    virtual bool supports_system_port_creation() const = 0;
    virtual std::shared_ptr<AudioPortInterface> create_system_audio_port(std::string name, PortDirection direction) = 0;
    virtual std::shared_ptr<MidiPortInterface> create_system_midi_port(std::string name, PortDirection direction) = 0;

    virtual void start() = 0;
    virtual uint32_t get_sample_rate() const = 0;
    virtual uint32_t get_buffer_size() const = 0;
    virtual void* maybe_client_handle() const = 0;
    virtual const char* client_name() const = 0;

    virtual void close() = 0;

    virtual uint32_t get_xruns() const = 0;
    virtual void reset_xruns() = 0;

    AudioSystemInterface() {}
    virtual ~AudioSystemInterface() {}
};