#pragma once
#include <memory>
#include <set>
#include "libshoopdaloop_backend.h"
#include "GraphPort.h"
#include "GraphLoopChannel.h"
#include "BackendSession.h"
#include "GraphLoop.h"
#include "GraphFXChain.h"
#include "types.h"
#include "shoop_globals.h"
#include "MidiChannel.h"

class BackendSession;

std::set<shoop_shared_ptr<BackendSession>> &get_active_backends();

shoop_shared_ptr<BackendSession> internal_backend_session(shoop_backend_session_t *backend);

shoop_shared_ptr<GraphPort> internal_audio_port(shoopdaloop_audio_port_t *port);

shoop_shared_ptr<GraphPort> internal_midi_port(shoopdaloop_midi_port_t *port);

shoop_shared_ptr<GraphLoopChannel> internal_audio_channel(shoopdaloop_loop_audio_channel_t *chan);

shoop_shared_ptr<GraphLoopChannel> internal_midi_channel(shoopdaloop_loop_midi_channel_t *chan);

shoop_shared_ptr<GraphLoop> internal_loop(shoopdaloop_loop_t *loop);

shoop_shared_ptr<GraphFXChain> internal_fx_chain(shoopdaloop_fx_chain_t *chain);

shoop_backend_session_t *external_backend_session(shoop_shared_ptr<BackendSession> backend);

shoopdaloop_audio_port_t* external_audio_port(shoop_shared_ptr<GraphPort> port);

shoopdaloop_midi_port_t* external_midi_port(shoop_shared_ptr<GraphPort> port);

shoopdaloop_loop_audio_channel_t* external_audio_channel(shoop_shared_ptr<GraphLoopChannel> port);

shoopdaloop_loop_midi_channel_t* external_midi_channel(shoop_shared_ptr<GraphLoopChannel> port);

shoopdaloop_loop_t* external_loop(shoop_shared_ptr<GraphLoop> loop);

shoopdaloop_fx_chain_t* external_fx_chain(shoop_shared_ptr<GraphFXChain> chain);

shoop_audio_channel_data_t *external_audio_data(std::vector<audio_sample_t> f);

shoop_midi_sequence_t *external_midi_data(shoop_types::LoopMidiChannel::Contents const& m);

std::vector<float> internal_audio_data(shoop_audio_channel_data_t const& d);

shoop_types::LoopMidiChannel::Contents internal_midi_data(shoop_midi_sequence_t const& d);

shoopdaloop_decoupled_midi_port_t *external_decoupled_midi_port(shoop_shared_ptr<_DecoupledMidiPort> port);

shoop_shared_ptr<_DecoupledMidiPort> internal_decoupled_midi_port(shoopdaloop_decoupled_midi_port_t *port);

shoop_shared_ptr<AudioMidiDriver> internal_audio_driver(shoop_audio_driver_t *driver);

shoop_audio_driver_t *external_audio_driver(shoop_shared_ptr<AudioMidiDriver> driver);