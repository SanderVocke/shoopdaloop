#pragma once
#include <memory>
#include "PortInterface.h"
#include "MidiPortInterface.h"
#include <string>
#include "AudioPortInterface.h"
#include <stdint.h>
#include "WithCommandQueue.h"
#include "types.h"
#include <set>
#include "shoop_globals.h"

enum class ProcessFunctionResult {
    Continue,  // Continue processing next cycle
    Disconnect // Request to be disconnected from the audio processing thread
};

struct AudioMidiDriverSettingsInterface {
    AudioMidiDriverSettingsInterface() {}
    virtual ~AudioMidiDriverSettingsInterface() {}
};

class HasAudioProcessingFunction {
public:
    HasAudioProcessingFunction() {}
    virtual ~HasAudioProcessingFunction() {}

    virtual void PROC_process(uint32_t nframes) = 0;
};

class AudioMidiDriver : public WithCommandQueue,
                        private std::enable_shared_from_this<AudioMidiDriver> {
    std::shared_ptr<std::set<HasAudioProcessingFunction*>> m_processors;
    std::atomic<uint32_t> m_xruns;
    std::atomic<uint32_t> m_sample_rate;
    std::atomic<uint32_t> m_buffer_size;
    std::atomic<float> m_dsp_load;
    std::atomic<void*> m_maybe_client_handle;
    std::atomic<const char*> m_client_name;
    std::set<std::shared_ptr<shoop_types::_DecoupledMidiPort>> m_decoupled_midi_ports;

protected:
    // Derived class should call these
    void report_xrun();
    void set_dsp_load(float load);
    void set_sample_rate(uint32_t sample_rate);
    void set_buffer_size(uint32_t buffer_size);
    void set_maybe_client_handle(void* handle);
    void set_client_name(const char* name);

    virtual void maybe_update_sample_rate() {};
    virtual void maybe_update_buffer_size() {};
    virtual void maybe_update_dsp_load() {};

    void PROC_process(uint32_t nframes);

public:
    void add_processor(HasAudioProcessingFunction &p);
    void remove_processor(HasAudioProcessingFunction &p);
    std::set<HasAudioProcessingFunction*> processors() const;

    virtual void start(AudioMidiDriverSettingsInterface &settings) = 0;

    virtual
    std::shared_ptr<AudioPortInterface<audio_sample_t>> open_audio_port(
        std::string name,
        shoop_port_direction_t direction
    ) = 0;

    virtual
    std::shared_ptr<MidiPortInterface> open_midi_port(
        std::string name,
        shoop_port_direction_t direction
    ) = 0;

    std::shared_ptr<shoop_types::_DecoupledMidiPort> open_decoupled_midi_port(
        std::string name,
        shoop_port_direction_t direction
    );
    
    void unregister_decoupled_midi_port(std::shared_ptr<shoop_types::_DecoupledMidiPort> port);

    virtual void close() = 0;

    uint32_t get_xruns() const;
    float get_dsp_load();
    uint32_t get_sample_rate();
    uint32_t get_buffer_size();
    void reset_xruns();
    const char* get_client_name() const;
    void* get_maybe_client_handle() const;

    AudioMidiDriver() : WithCommandQueue() {}
    virtual ~AudioMidiDriver() {}
};