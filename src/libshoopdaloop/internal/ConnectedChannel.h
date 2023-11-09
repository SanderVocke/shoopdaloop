#pragma once
#include <memory>
#include <vector>
#include <atomic>
#include "shoop_globals.h"
#include "process_when.h"

class ChannelInterface;
class ConnectedLoop;
class ConnectedPort;
class Backend;
template<typename SampleT> class AudioChannel;
template<typename A, typename B> class MidiChannel;

struct ConnectedChannel : public std::enable_shared_from_this<ConnectedChannel> {
    std::shared_ptr<ChannelInterface> channel;
    std::weak_ptr<ConnectedLoop> loop;
    std::weak_ptr<ConnectedPort> mp_input_port_mapping;
    std::weak_ptr<Backend> backend;
    std::weak_ptr<ConnectedPort> mp_output_port_mapping;
    shoop_types::ProcessWhen ma_process_when;
    std::atomic<unsigned> ma_data_sequence_nr;

    ConnectedChannel(std::shared_ptr<ChannelInterface> chan,
                std::shared_ptr<ConnectedLoop> loop,
                std::shared_ptr<Backend> backend);

    // NOTE: only use on process thread
    ConnectedChannel &operator= (ConnectedChannel const& other);

    void connect_output_port(std::shared_ptr<ConnectedPort> port, bool thread_safe=true);
    void connect_input_port(std::shared_ptr<ConnectedPort> port, bool thread_safe=true);
    void disconnect_output_port(std::shared_ptr<ConnectedPort> port, bool thread_safe=true);
    void disconnect_output_ports(bool thread_safe=true);
    void disconnect_input_port(std::shared_ptr<ConnectedPort> port, bool thread_safe=true);
    void disconnect_input_ports(bool thread_safe=true);
    void PROC_prepare_process_audio(uint32_t n_frames);
    void PROC_prepare_process_midi(uint32_t n_frames);
    void PROC_finalize_process_audio();
    void PROC_finalize_process_midi();
    Backend &get_backend();
    void clear_data_dirty();
    bool get_data_dirty() const;
    shoop_types::LoopAudioChannel* maybe_audio();
    shoop_types::LoopMidiChannel* maybe_midi();
};