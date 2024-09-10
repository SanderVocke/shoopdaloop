#include "GraphLoop.h"
#include "GraphLoopChannel.h"
#include "AudioMidiLoop.h"
#include "BackendSession.h"
#include "GraphNode.h"
#include "LoopInterface.h"
#include "process_loops.h"
#include "types.h"
#include <memory>
#include <fmt/format.h>

void GraphLoop::PROC_prepare(uint32_t n_frames) {
    for (auto &chan : mp_audio_channels) {
        chan->PROC_prepare(n_frames);
    }
    for (auto &chan : mp_midi_channels) {
        chan->PROC_prepare(n_frames);
    }
}

void GraphLoop::PROC_process(uint32_t n_frames) {
    for (auto &chan : mp_audio_channels) {
        chan->PROC_process(n_frames);
    }
    for (auto &chan : mp_midi_channels) {
        chan->PROC_process(n_frames);
    }
}

void GraphLoop::delete_audio_channel_idx(uint32_t idx, bool thread_safe) {
    auto chaninfo = mp_audio_channels.at(idx);
    delete_midi_channel(chaninfo, false);
    get_backend().set_graph_node_changes_pending();
}

void GraphLoop::delete_midi_channel_idx(uint32_t idx, bool thread_safe) {
    auto chaninfo = mp_midi_channels.at(idx);
    delete_midi_channel(chaninfo, false);
    get_backend().set_graph_node_changes_pending();
}

void GraphLoop::delete_audio_channel(shoop_shared_ptr<GraphLoopChannel> chan, bool thread_safe) {
    auto r = std::find_if(mp_audio_channels.begin(), mp_audio_channels.end(), [chan](auto const& e) { return chan->channel.get() == e->channel.get(); });
    if (r == mp_audio_channels.end()) {
        throw std::runtime_error("Attempting to delete non-existent audio channel.");
    }
    loop->delete_audio_channel(chan->channel, thread_safe);
    mp_audio_channels.erase(r);
    get_backend().set_graph_node_changes_pending();
}

void GraphLoop::delete_midi_channel(shoop_shared_ptr<GraphLoopChannel> chan, bool thread_safe) {
    auto r = std::find_if(mp_midi_channels.begin(), mp_midi_channels.end(), [chan](auto const& e) { return chan->channel.get() == e->channel.get(); });
    if (r == mp_midi_channels.end()) {
        throw std::runtime_error("Attempting to delete non-existent midi channel.");
    }
    loop->delete_midi_channel(chan->channel, thread_safe);
    mp_midi_channels.erase(r);
    get_backend().set_graph_node_changes_pending();
}

void GraphLoop::delete_all_channels(bool thread_safe) {
    for (auto &chan : mp_audio_channels) {
        loop->delete_audio_channel(chan->channel, thread_safe);
    }
    mp_audio_channels.clear();
    for (auto &chan : mp_midi_channels) {
        loop->delete_midi_channel(chan->channel, thread_safe);
    }
    mp_midi_channels.clear();
}

BackendSession &GraphLoop::get_backend() {
    auto b = backend.lock();
    if(!b) {
        throw std::runtime_error("Back-end no longer exists");
    }
    return *b;
}

void GraphLoop::graph_node_co_process(std::set<shoop_shared_ptr<GraphNode>> const& nodes, uint32_t nframes) {
    process_loops<std::set<shoop_shared_ptr<GraphNode>>::iterator>(
        nodes.begin(), nodes.end(), nframes,
        [](std::set<shoop_shared_ptr<GraphNode>>::iterator &node_it) -> LoopInterface* {
            shoop_shared_ptr<GraphLoop> l = graph_node_parent_as<GraphLoop, HasGraphNode>(**node_it);
            if (l) {
                return l->loop.get();
            }
            return nullptr;
        });
}

void GraphLoop::graph_node_process(uint32_t nframes) {
    std::array<shoop_shared_ptr<GraphLoop>, 1> loops;
    loops[0] = shoop_static_pointer_cast<GraphLoop>(shared_from_this());
    process_loops<decltype(loops)::iterator>(
        loops.begin(), loops.end(), nframes,
        [](decltype(loops)::iterator &node) -> LoopInterface* {
            return (*node)->loop.get();
        });
}

WeakGraphNodeSet GraphLoop::graph_node_co_process_nodes() {
    if (m_get_co_process_nodes) { return m_get_co_process_nodes(); }
    return WeakGraphNodeSet();
}

void GraphLoop::PROC_adopt_ringbuffer_contents(
    std::optional<unsigned> reverse_cycles_start,
    std::optional<unsigned> cycles_length,
    std::optional<unsigned> go_to_cycle,
    shoop_loop_mode_t go_to_mode
) {
    std::optional<unsigned> reverse_start_offset = std::nullopt;

    auto sync_source = loop->get_sync_source(false);
    auto sync_len = sync_source ? sync_source->get_length() : 0;
    auto sync_pos = sync_source ? sync_source->get_position() : 0;
    log<log_level_debug>("adopt ringbuffer: reverse start {}, {} cycles, go to cycle {}, go to mode {}, sync pos {}, sync len {}",
        reverse_cycles_start.has_value() ? (int)reverse_cycles_start.value() : -1,
        cycles_length.has_value() ? (int)cycles_length.value() : -1,
        go_to_cycle.has_value() ? (int)go_to_cycle.value() : -1,
        (int)go_to_mode, sync_pos, sync_len);
    if (sync_len > 0 && reverse_cycles_start.has_value() && cycles_length.has_value()) {
        reverse_start_offset = sync_pos + sync_len * reverse_cycles_start.value();
    } else if (sync_len > 0 && cycles_length.has_value() && cycles_length.value() > 0) {
        reverse_start_offset = sync_pos + sync_len * (cycles_length.value() - 1);
    }

    // Keep an additional cycle of data before the start offset to allow some flexibility
    unsigned keep_before_start_offset = sync_len;
    unsigned max_data_length = 0;
    for (auto &c : mp_audio_channels) { c->adopt_ringbuffer_contents(reverse_start_offset, keep_before_start_offset, false); max_data_length = std::max(max_data_length, c->channel->get_length()); }
    for (auto &c : mp_midi_channels)  { c->adopt_ringbuffer_contents(reverse_start_offset, keep_before_start_offset, false); /* max_data_length = std::max(max_data_length, c->channel->get_length()); */ }

    unsigned samples_length = max_data_length;
    if (cycles_length.has_value() && sync_len > 0) {
        samples_length = std::min(samples_length, cycles_length.value() * sync_len);
    }
    if (reverse_start_offset.has_value()) {
        samples_length = std::min(samples_length, reverse_start_offset.value());
    }

    loop->set_length(samples_length, false);
    if (go_to_mode != LoopMode_Unknown) {
        loop->plan_transition(go_to_mode, std::nullopt, go_to_cycle.value_or(0), false);
    }
}