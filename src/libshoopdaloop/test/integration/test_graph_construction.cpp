#include "AudioMidiLoop.h"
#include "BackendSession.h"
#include "BasicLoop.h"
#include "AudioPort.h"
#include "GraphLoopChannel.h"
#include "GraphLoop.h"
#include "GraphAudioPort.h"
#include "DummyAudioMidiDriver.h"
#include "GraphNode.h"
#include "PortInterface.h"
#include "graph_processing_order.h"
#include "shoop_globals.h"
#include "types.h"
#include <catch2/catch_test_macros.hpp>
#include <algorithm>

template <typename NodesType>
void insert_all(NodesType &n, std::set<GraphNode *> &nodes) {
    for (auto &item : n.all_graph_nodes()) {
        nodes.insert(item.get());
    }
};

class TestPort : public AudioPort<float> {
  public:
    std::string m_name;
    std::vector<float> m_buffer;

    void close() override {}
    const char *name() const override { return m_name.c_str(); }
    PortDataType type() const override { return PortDataType::Audio; }
    PortExternalConnectionStatus
    get_external_connection_status() const override {
        return PortExternalConnectionStatus();
    }
    void connect_external(std::string name) override {}
    void disconnect_external(std::string name) override {}

    bool has_internal_read_access() const override { return true; }
    bool has_internal_write_access() const override { return true; };
    bool has_implicit_input_source() const override { return true; };
    bool has_implicit_output_sink() const override { return true; };

    void *maybe_driver_handle() const override { return nullptr; }

    void PROC_prepare(uint32_t nframes) override {}
    void PROC_process(uint32_t nframes) override {}

    bool get_muted() const override { return false; }
    void set_muted(bool muted) override {}

    float* PROC_get_buffer(uint32_t nframes) override {
        m_buffer.resize(std::max((size_t)nframes, m_buffer.size()));
        return m_buffer.data();
    }

    TestPort(std::string name) : PortInterface(), m_name(name) {}
    ~TestPort(){};
};

std::vector<std::vector<std::string>>
get_names(std::vector<std::set<GraphNode *>> const &schedule) {
    std::vector<std::vector<std::string>> result;
    for (auto &s : schedule) {
        std::vector<std::string> names;
        for (auto &n : s) {
            names.push_back(n->graph_node_name());
        }
        std::sort(names.begin(), names.end());
        result.push_back(names);
    }
    return result;
}

TEST_CASE("Graph Construction - Two Ports", "[GraphConstruct]") {
    auto backend = std::make_shared<BackendSession>();
    auto _p1 = std::make_shared<TestPort>("p1");
    auto _p2 = std::make_shared<TestPort>("p2");
    auto port1 = std::make_shared<GraphAudioPort>(_p1, backend);
    auto port2 = std::make_shared<GraphAudioPort>(_p2, backend);
    port1->connect_passthrough(port2);

    std::set<GraphNode *> nodes;
    insert_all(*port1, nodes);
    insert_all(*port2, nodes);
    auto schedule = graph_processing_order(nodes);
    auto schedule_names = get_names(schedule);

    std::vector<std::vector<std::string>> expected = {
        {"p1::prepare"},
        {"p2::prepare"},
        {"p1::process_and_passthrough"},
        {"p2::process_and_passthrough"}};

    CHECK(get_names(schedule) == expected);
};

TEST_CASE("Graph Construction - Direct Loop", "[GraphConstruct]") {
    auto backend = std::make_shared<BackendSession>();
    auto _p1 = std::make_shared<TestPort>("p1");
    auto _p2 = std::make_shared<TestPort>("p2");
    auto port1 = std::make_shared<GraphAudioPort>(_p1, backend);
    auto port2 = std::make_shared<GraphAudioPort>(_p2, backend);
    port1->connect_passthrough(port2);
    auto loop = backend->create_loop();
    auto chan = std::make_shared<GraphLoopChannel>(nullptr, loop, backend);
    chan->connect_input_port(port1, false);
    chan->connect_output_port(port2, false);

    std::set<GraphNode *> nodes;
    insert_all(*port1, nodes);
    insert_all(*port2, nodes);
    insert_all(*chan, nodes);
    insert_all(*loop, nodes);
    auto schedule = graph_processing_order(nodes);
    auto schedule_names = get_names(schedule);

    std::vector<std::vector<std::string>> expected = {
        {"p1::prepare"},
        {"p2::prepare"},
        {"channel::prepare_buffers"},
        {"p1::process_and_passthrough"},
        {"loop::process"},
        {"channel::process"},
        {"p2::process_and_passthrough"}};

    CHECK(get_names(schedule) == expected);
};

TEST_CASE("Graph Construction - Two Direct Loops", "[GraphConstruct]") {
    auto backend = std::make_shared<BackendSession>();
    auto _p1 = std::make_shared<TestPort>("p1");
    auto _p2 = std::make_shared<TestPort>("p2");
    auto port1 = std::make_shared<GraphAudioPort>(_p1, backend);
    auto port2 = std::make_shared<GraphAudioPort>(_p2, backend);
    port1->connect_passthrough(port2);
    auto loop1 = backend->create_loop();
    auto chan1 = std::make_shared<GraphLoopChannel>(nullptr, loop1, backend);
    auto loop2 = backend->create_loop();
    auto chan2 = std::make_shared<GraphLoopChannel>(nullptr, loop2, backend);
    WeakGraphNodeSet loops { loop1->graph_node(), loop2->graph_node() };
    loop1->set_get_co_process_nodes_cb([&]() { return loops; });
    loop2->set_get_co_process_nodes_cb([&]() { return loops; });
    chan1->connect_input_port(port1, false);
    chan1->connect_output_port(port2, false);
    chan2->connect_input_port(port1, false);
    chan2->connect_output_port(port2, false);

    std::set<GraphNode *> nodes;
    insert_all(*port1, nodes);
    insert_all(*port2, nodes);
    insert_all(*chan1, nodes);
    insert_all(*loop1, nodes);
    insert_all(*chan2, nodes);
    insert_all(*loop2, nodes);
    auto schedule = graph_processing_order(nodes);
    auto schedule_names = get_names(schedule);

    std::vector<std::vector<std::string>> expected = {
        {"p1::prepare"},
        {"p2::prepare"},
        {"channel::prepare_buffers"},
        {"channel::prepare_buffers"},
        {"p1::process_and_passthrough"},
        {"loop::process", "loop::process"},
        {"channel::process"},
        {"channel::process"},
        {"p2::process_and_passthrough"}};

    CHECK(get_names(schedule) == expected);
};
