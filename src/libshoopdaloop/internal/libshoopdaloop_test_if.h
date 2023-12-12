#pragma once
#include <memory>
#include <set>
#include "libshoopdaloop.h"
#include "GraphPort.h"
#include "ConnectedChannel.h"
#include "BackendSession.h"
#include "ConnectedLoop.h"
#include "ConnectedFXChain.h"
#include "types.h"
#include "shoop_globals.h"

class BackendSession;

std::set<std::shared_ptr<BackendSession>> &get_active_backends();

std::shared_ptr<BackendSession> internal_backend_session(shoop_backend_session_t *backend);

std::shared_ptr<GraphPort> internal_audio_port(shoopdaloop_audio_port_t *port);

std::shared_ptr<GraphPort> internal_midi_port(shoopdaloop_midi_port_t *port);

std::shared_ptr<ConnectedChannel> internal_audio_channel(shoopdaloop_loop_audio_channel_t *chan);

std::shared_ptr<ConnectedChannel> internal_midi_channel(shoopdaloop_loop_midi_channel_t *chan);

std::shared_ptr<ConnectedLoop> internal_loop(shoopdaloop_loop_t *loop);

std::shared_ptr<ConnectedFXChain> internal_fx_chain(shoopdaloop_fx_chain_t *chain);

shoop_backend_session_t *external_backend_session(std::shared_ptr<BackendSession> backend);

shoopdaloop_audio_port_t* external_audio_port(std::shared_ptr<GraphPort> port);

shoopdaloop_midi_port_t* external_midi_port(std::shared_ptr<GraphPort> port);

shoopdaloop_loop_audio_channel_t* external_audio_channel(std::shared_ptr<ConnectedChannel> port);

shoopdaloop_loop_midi_channel_t* external_midi_channel(std::shared_ptr<ConnectedChannel> port);

shoopdaloop_loop_t* external_loop(std::shared_ptr<ConnectedLoop> loop);

shoopdaloop_fx_chain_t* external_fx_chain(std::shared_ptr<ConnectedFXChain> chain);

shoop_audio_channel_data_t *external_audio_data(std::vector<audio_sample_t> f);

shoop_midi_sequence_t *external_midi_data(std::vector<_MidiMessage> m);

std::vector<float> internal_audio_data(shoop_audio_channel_data_t const& d);

std::vector<_MidiMessage> internal_midi_data(shoop_midi_sequence_t const& d);

shoopdaloop_decoupled_midi_port_t *external_decoupled_midi_port(std::shared_ptr<_DecoupledMidiPort> port);

std::shared_ptr<_DecoupledMidiPort> internal_decoupled_midi_port(shoopdaloop_decoupled_midi_port_t *port);

std::shared_ptr<AudioMidiDriver> internal_audio_driver(shoop_audio_driver_t *driver);

shoop_audio_driver_t *external_audio_driver(std::shared_ptr<AudioMidiDriver> driver);