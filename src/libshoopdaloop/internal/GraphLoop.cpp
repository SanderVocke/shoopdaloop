#include "GraphLoop.h"
#include "GraphLoopChannel.h"
#include "AudioMidiLoop.h"
#include "BackendSession.h"
#include "GraphNode.h"
#include "LoopInterface.h"
#include "process_loops.h"
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

void GraphLoop::delete_audio_channel(std::shared_ptr<GraphLoopChannel> chan, bool thread_safe) {
    auto r = std::find_if(mp_audio_channels.begin(), mp_audio_channels.end(), [chan](auto const& e) { return chan->channel.get() == e->channel.get(); });
    if (r == mp_audio_channels.end()) {
        throw std::runtime_error("Attempting to delete non-existent audio channel.");
    }
    loop->delete_audio_channel(chan->channel, thread_safe);
    mp_audio_channels.erase(r);
    get_backend().set_graph_node_changes_pending();
}

void GraphLoop::delete_midi_channel(std::shared_ptr<GraphLoopChannel> chan, bool thread_safe) {
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

void GraphLoop::graph_node_co_process(std::set<std::shared_ptr<GraphNode>> const& nodes, uint32_t nframes) {
    process_loops<std::set<std::shared_ptr<GraphNode>>::iterator>(
        nodes.begin(), nodes.end(), nframes,
        [](std::set<std::shared_ptr<GraphNode>>::iterator &node_it) -> LoopInterface* {
            std::shared_ptr<GraphLoop> l = graph_node_parent_as<GraphLoop, HasGraphNode>(**node_it);
            if (l) {
                return l->loop.get();
            }
            return nullptr;
        });
}

void GraphLoop::graph_node_process(uint32_t nframes) {
    std::array<std::shared_ptr<GraphLoop>, 1> loops;
    loops[0] = static_pointer_cast<GraphLoop>(shared_from_this());
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

void GraphLoop::PROC_adopt_ringbuffer_contents(unsigned reverse_cycles_start, unsigned cycles_length) {
    //loop->adopt_ringbuffer_contents(std::shared_ptr<shoop_types::_AudioPort> from_port)

    std::optional<unsigned> reverse_start_offset = std::nullopt;
    unsigned samples_length = 0;
    auto sync_source = loop->get_sync_source();
    if (sync_source) {
        auto len = sync_source->get_length();
        auto cur_cycle_reverse_start = sync_source->get_position();
        reverse_start_offset = cur_cycle_reverse_start + len * reverse_cycles_start;
    }

    for (auto &c : mp_audio_channels) { c->adopt_ringbuffer_contents(reverse_start_offset, false); }
    for (auto &c : mp_midi_channels)  { c->adopt_ringbuffer_contents(reverse_start_offset, false); }

}