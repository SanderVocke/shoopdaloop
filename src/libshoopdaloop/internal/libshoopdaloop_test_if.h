#pragma once
#include <memory>
#include <set>
#include "libshoopdaloop.h"
#include "ConnectedPort.h"
#include "ConnectedChannel.h"
#include "Backend.h"
#include "ConnectedLoop.h"
#include "ConnectedFXChain.h"
#include "ConnectedDecoupledMidiPort.h"
#include "ObjectPool.h"

class Backend;

std::set<std::shared_ptr<Backend>> &get_active_backends();

std::shared_ptr<Backend> internal_backend(shoopdaloop_backend_instance_t *backend);

std::shared_ptr<ConnectedPort> internal_audio_port(shoopdaloop_audio_port_t *port);

std::shared_ptr<ConnectedPort> internal_midi_port(shoopdaloop_midi_port_t *port);

std::shared_ptr<ConnectedChannel> internal_audio_channel(shoopdaloop_loop_audio_channel_t *chan);

std::shared_ptr<ConnectedChannel> internal_midi_channel(shoopdaloop_loop_midi_channel_t *chan);

std::shared_ptr<ConnectedLoop> internal_loop(shoopdaloop_loop_t *loop);

std::shared_ptr<ConnectedFXChain> internal_fx_chain(shoopdaloop_fx_chain_t *chain);

shoopdaloop_backend_instance_t *external_backend(std::shared_ptr<Backend> backend);

shoopdaloop_audio_port_t* external_audio_port(std::shared_ptr<ConnectedPort> port);

shoopdaloop_midi_port_t* external_midi_port(std::shared_ptr<ConnectedPort> port);

shoopdaloop_loop_audio_channel_t* external_audio_channel(std::shared_ptr<ConnectedChannel> port);

shoopdaloop_loop_midi_channel_t* external_midi_channel(std::shared_ptr<ConnectedChannel> port);

shoopdaloop_loop_t* external_loop(std::shared_ptr<ConnectedLoop> loop);

shoopdaloop_fx_chain_t* external_fx_chain(std::shared_ptr<ConnectedFXChain> chain);

audio_channel_data_t *external_audio_data(std::vector<audio_sample_t> f);

midi_sequence_t *external_midi_data(std::vector<_MidiMessage> m);

std::vector<float> internal_audio_data(audio_channel_data_t const& d);

std::vector<_MidiMessage> internal_midi_data(midi_sequence_t const& d);

shoopdaloop_decoupled_midi_port_t *external_decoupled_midi_port(std::shared_ptr<ConnectedDecoupledMidiPort> port);

std::shared_ptr<ConnectedDecoupledMidiPort> internal_decoupled_midi_port(shoopdaloop_decoupled_midi_port_t *port);

PortDirection internal_port_direction(port_direction_t d);