#pragma once
#include <memory>
#include <vector>
#include "shoop_globals.h"
#include "GraphNode.h"

class AudioMidiLoop;
class GraphLoopChannel;
class BackendSession;

class GraphLoop : public std::enable_shared_from_this<GraphLoop>,
                      public HasGraphNode {
public:

    const std::shared_ptr<AudioMidiLoop> loop;
    WeakGraphNodeSet m_other_loops;
    std::vector<std::shared_ptr<GraphLoopChannel>> mp_audio_channels;
    std::vector<std::shared_ptr<GraphLoopChannel>>  mp_midi_channels;
    std::weak_ptr<BackendSession> backend;
    std::function<WeakGraphNodeSet()> m_get_co_process_nodes;

    GraphLoop(std::shared_ptr<BackendSession> backend,
             std::shared_ptr<AudioMidiLoop> loop) :
        loop(loop),
        backend(backend)
    {
        mp_audio_channels.reserve(shoop_constants::default_max_audio_channels);
        mp_midi_channels.reserve(shoop_constants::default_max_midi_channels);
    }

    void delete_audio_channel_idx(uint32_t idx, bool thread_safe=true);
    void delete_midi_channel_idx(uint32_t idx, bool thread_safe=true);
    void delete_audio_channel(std::shared_ptr<GraphLoopChannel> chan, bool thread_safe=true);
    void delete_midi_channel(std::shared_ptr<GraphLoopChannel> chan, bool thread_safe=true);
    void delete_all_channels(bool thread_safe=true);
    void PROC_prepare(uint32_t n_frames);
    void PROC_process(uint32_t n_frames);

    BackendSession &get_backend();

    void set_get_co_process_nodes_cb(std::function<WeakGraphNodeSet()> cb) {
        m_get_co_process_nodes = cb;
    }

    // Graph node connections are all handled by the channel nodes, so
    // we don't need to connect anything. Just define the processing function
    // that allows us to process all loops together.
    void graph_node_co_process(std::set<std::shared_ptr<GraphNode>> const& nodes, uint32_t nframes) override;
    void graph_node_process(uint32_t nframes) override;
    std::string graph_node_name() const override { return "loop::process"; }
    WeakGraphNodeSet graph_node_co_process_nodes() override;
};