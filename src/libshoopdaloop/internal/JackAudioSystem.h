#pragma once
#include "AudioSystemInterface.h"
#include <jack/types.h>
#include <map>
#include <atomic>

class JackAudioSystem : public AudioSystemInterface<jack_nframes_t, size_t> {
    jack_client_t * m_client;
    std::string m_client_name;
    size_t m_sample_rate;
    std::map<std::string, std::shared_ptr<PortInterface>> m_ports;
    std::function<void(size_t)> m_process_cb;
    std::atomic<unsigned> m_xruns = 0;

    static int PROC_process_cb_static (jack_nframes_t nframes,
                                  void *arg);

    static int PROC_xrun_cb_static(void *arg);

    int PROC_process_cb_inst (jack_nframes_t nframes);

    int PROC_xrun_cb_inst ();

public:
    JackAudioSystem(
        std::string client_name,
        std::function<void(size_t)> process_cb
    );

    void start() override;

    ~JackAudioSystem() override;

    std::shared_ptr<AudioPortInterface<float>> open_audio_port(
        std::string name,
        PortDirection direction
    ) override;

    std::shared_ptr<MidiPortInterface> open_midi_port(
        std::string name,
        PortDirection direction
    ) override;

    size_t get_sample_rate() const override;
    size_t get_buffer_size() const override;
    void* maybe_client_handle() const override;
    const char* client_name() const override;
    void close() override;
    size_t get_xruns() const override;
    void reset_xruns() override;
};