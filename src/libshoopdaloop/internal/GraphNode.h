#pragma once
#include <set>
#include <memory>
#include <string>
#include <vector>
#include <functional>
#include <iterator>
#include <chrono>

class GraphNode;

using SharedGraphNodeSet = std::set<std::shared_ptr<GraphNode>>;
using WeakGraphNodeSet = std::set<std::weak_ptr<GraphNode>, std::owner_less<std::weak_ptr<GraphNode>>>;

class GraphNode : public std::enable_shared_from_this<GraphNode> {
    std::function<void(uint32_t)> m_processed_cb = nullptr; // arg is process time in us
public:
    GraphNode() {};

    // The graph processing algorithm will combine all edges - it is not necessary for both sides of
    // an edge to be annotated. The only reason we have both outgoing and incoming edges here is to
    // allow the derived class to choose whatever is more convenient.
    virtual WeakGraphNodeSet graph_node_outgoing_edges() { return WeakGraphNodeSet(); };
    virtual WeakGraphNodeSet graph_node_incoming_edges() { return WeakGraphNodeSet(); };
    virtual WeakGraphNodeSet graph_node_co_process_nodes() { return WeakGraphNodeSet(); };
    
    // For debugging purposes.
    virtual std::string graph_node_name() const { return "GraphNode"; }

    // Processing implementation to be overridden by children.
    virtual void graph_node_process(uint32_t nframes) {}

    // Process and time the processing.
    void PROC_process(uint32_t nframes) {
        if (m_processed_cb) {
            auto start = std::chrono::high_resolution_clock::now();
            graph_node_process(nframes);
            auto end = std::chrono::high_resolution_clock::now();
            auto duration = std::chrono::duration_cast<std::chrono::microseconds>(end - start);
            m_processed_cb(duration.count());
        } else {
            graph_node_process(nframes);
        }
    }

    // Function to call in order to process this node with its co-process nodes.
    virtual void graph_node_co_process(std::set<std::shared_ptr<GraphNode>> const& nodes,
        uint32_t nframes) {}
    
    // Process and time the processing.
    void PROC_co_process(std::set<std::shared_ptr<GraphNode>> const& nodes,
        uint32_t nframes) {
        if (m_processed_cb) {
            auto start = std::chrono::high_resolution_clock::now();
            graph_node_co_process(nodes, nframes);
            auto end = std::chrono::high_resolution_clock::now();
            auto duration = std::chrono::duration_cast<std::chrono::microseconds>(end - start);
            m_processed_cb(duration.count());
        } else {
            graph_node_co_process(nodes, nframes);
        }
    }
    
    // Callback for profiling.
    void set_processed_cb(std::function<void(uint32_t)> cb) { m_processed_cb = cb; }
};

template<typename Parent>
class NodeWithParent : public GraphNode {
    protected:
    std::weak_ptr<Parent> m_parent = nullptr;
    NodeWithParent(std::weak_ptr<Parent> parent) : m_parent(parent) {};

    public:
    std::weak_ptr<Parent> parent() { return m_parent; }
};

template<typename Target, typename Parent>
std::shared_ptr<Target> graph_node_parent_as(GraphNode &node) {
    auto converted = static_cast<NodeWithParent<Parent>&>(node);
    return std::dynamic_pointer_cast<Target>(converted.parent().lock());
}

class NotifyProcessParametersInterface {
public:
    virtual void PROC_notify_changed_buffer_size(uint32_t buffer_size) {}
    virtual void PROC_notify_changed_sample_rate(uint32_t sample_rate) {}
};

class HasGraphNodesInterface : public NotifyProcessParametersInterface {
public:
    HasGraphNodesInterface() {}

    virtual SharedGraphNodeSet all_graph_nodes() { return SharedGraphNodeSet(); }
};
class HasGraphNode : public HasGraphNodesInterface, public std::enable_shared_from_this<HasGraphNode> {
    class Node : public NodeWithParent<HasGraphNode> {
    public:
        Node(std::weak_ptr<HasGraphNode> parent) : NodeWithParent<HasGraphNode>(parent) {};
        std::string graph_node_name() const override {
            if (auto parent = m_parent.lock()) {
                return parent->graph_node_name();
            }
            return "expired";
        }
        WeakGraphNodeSet graph_node_outgoing_edges() override {
            if (auto parent = m_parent.lock()) {
                return parent->graph_node_outgoing_edges();
            }
            return WeakGraphNodeSet();
        }
        WeakGraphNodeSet graph_node_incoming_edges() override {
            if (auto parent = m_parent.lock()) {
                return parent->graph_node_incoming_edges();
            }
            return WeakGraphNodeSet();
        }
        WeakGraphNodeSet graph_node_co_process_nodes() override {
            if (auto parent = m_parent.lock()) {
                return parent->graph_node_co_process_nodes();
            }
            return WeakGraphNodeSet();
        }
        void graph_node_process(uint32_t nframes) override {
            if (auto parent = m_parent.lock()) {
                parent->graph_node_process(nframes);
            }
        }
        void graph_node_co_process(std::set<std::shared_ptr<GraphNode>> const& nodes,
            uint32_t nframes) override {
            if (auto parent = m_parent.lock()) {
                parent->graph_node_co_process(nodes, nframes);
            }
        }
    };
    std::shared_ptr<Node> m_node = nullptr;

    public:
    HasGraphNode() {};

    // Has the same interface as an actual GraphNode
    virtual std::string graph_node_name() const { return "GraphNode"; }
    virtual WeakGraphNodeSet graph_node_outgoing_edges() { return WeakGraphNodeSet(); };
    virtual WeakGraphNodeSet graph_node_incoming_edges() { return WeakGraphNodeSet(); };
    virtual WeakGraphNodeSet graph_node_co_process_nodes() { return WeakGraphNodeSet(); };
    virtual void graph_node_process(uint32_t nframes) {}
    virtual void graph_node_co_process(std::set<std::shared_ptr<GraphNode>> const& nodes,
        uint32_t nframes) {}

    SharedGraphNodeSet all_graph_nodes() override {
        if (!m_node) { m_node = std::make_shared<Node>(weak_from_this()); }
        return SharedGraphNodeSet({m_node});
    }
    std::shared_ptr<GraphNode> graph_node() {
        if (!m_node) { m_node = std::make_shared<Node>(weak_from_this()); }
        return m_node;
    }
};

class HasTwoGraphNodes : public HasGraphNodesInterface, public std::enable_shared_from_this<HasTwoGraphNodes> {
    class FirstNode : public NodeWithParent<HasTwoGraphNodes> {
    public:
        FirstNode(std::weak_ptr<HasTwoGraphNodes> parent) : NodeWithParent<HasTwoGraphNodes>(parent) {};
        std::string graph_node_name() const override {
            if (auto parent = m_parent.lock()) {
                return parent->graph_node_0_name();
            }
            return "expired";
        }
        WeakGraphNodeSet graph_node_outgoing_edges() override {
            if (auto parent = m_parent.lock()) {
                return parent->graph_node_0_outgoing_edges();
            }
            return WeakGraphNodeSet();
        }
        WeakGraphNodeSet graph_node_incoming_edges() override {
            if (auto parent = m_parent.lock()) {
                return parent->graph_node_0_incoming_edges();
            }
            return WeakGraphNodeSet();
        }
        WeakGraphNodeSet graph_node_co_process_nodes() override {
            if (auto parent = m_parent.lock()) {
                return parent->graph_node_0_co_process_nodes();
            }
            return WeakGraphNodeSet();
        }
        void graph_node_process(uint32_t nframes) override {
            if (auto parent = m_parent.lock()) {
                parent->graph_node_0_process(nframes);
            }
        }
        void graph_node_co_process(std::set<std::shared_ptr<GraphNode>> const& nodes,
            uint32_t nframes) override {
            if (auto parent = m_parent.lock()) {
                parent->graph_node_0_co_process(nodes, nframes);
            }
        }
    };

    class SecondNode : public NodeWithParent<HasTwoGraphNodes> {
    public:
        SecondNode(std::weak_ptr<HasTwoGraphNodes> parent) : NodeWithParent<HasTwoGraphNodes>(parent) {};
        std::string graph_node_name() const override {
            if (auto parent = m_parent.lock()) {
                return parent->graph_node_1_name();
            }
            return "expired";
        }
        WeakGraphNodeSet graph_node_outgoing_edges() override {
            if (auto parent = m_parent.lock()) {
                return parent->graph_node_1_outgoing_edges();
            }
            return WeakGraphNodeSet();
        }
        WeakGraphNodeSet graph_node_incoming_edges() override {
            if (auto parent = m_parent.lock()) {
                return parent->graph_node_1_incoming_edges();
            }
            return WeakGraphNodeSet();
        }
        WeakGraphNodeSet graph_node_co_process_nodes() override {
            if (auto parent = m_parent.lock()) {
                return parent->graph_node_1_co_process_nodes();
            }
            return WeakGraphNodeSet();
        }
        void graph_node_process(uint32_t nframes) override {
            if (auto parent = m_parent.lock()) {
                parent->graph_node_1_process(nframes);
            }
        }
        void graph_node_co_process(std::set<std::shared_ptr<GraphNode>> const& nodes,
            uint32_t nframes) override {
            if (auto parent = m_parent.lock()) {
                parent->graph_node_1_co_process(nodes, nframes);
            }
        }
    };
    std::shared_ptr<FirstNode> m_firstnode = nullptr;
    std::shared_ptr<SecondNode> m_secondnode = nullptr;

    public:
    HasTwoGraphNodes() {}

    // Has the same interface as an actual GraphNode, but twice
    virtual std::string graph_node_0_name() const { return "GraphNode"; }
    virtual WeakGraphNodeSet graph_node_0_outgoing_edges() { return WeakGraphNodeSet(); };
    virtual WeakGraphNodeSet graph_node_0_incoming_edges() { return WeakGraphNodeSet(); };
    virtual WeakGraphNodeSet graph_node_0_co_process_nodes() { return WeakGraphNodeSet(); };
    virtual void graph_node_0_process(uint32_t nframes) {}
    virtual void graph_node_0_co_process(std::set<std::shared_ptr<GraphNode>> const& nodes,
        uint32_t nframes) {}
    
    virtual std::string graph_node_1_name() const { return "GraphNode"; }
    virtual WeakGraphNodeSet graph_node_1_outgoing_edges() { return WeakGraphNodeSet(); };
    virtual WeakGraphNodeSet graph_node_1_incoming_edges() { return WeakGraphNodeSet(); };
    virtual WeakGraphNodeSet graph_node_1_co_process_nodes() { return WeakGraphNodeSet(); };
    virtual void graph_node_1_process(uint32_t nframes) {}
    virtual void graph_node_1_co_process(std::set<std::shared_ptr<GraphNode>> const& nodes,
        uint32_t nframes) {}

    inline void ensure_nodes() {
        if (!m_firstnode) { m_firstnode = std::make_shared<FirstNode>(weak_from_this()); }
        if (!m_secondnode) { m_secondnode = std::make_shared<SecondNode>(weak_from_this()); }
    }

    SharedGraphNodeSet all_graph_nodes() override {
        ensure_nodes();
        return SharedGraphNodeSet({m_firstnode, m_secondnode});
    }
    std::shared_ptr<GraphNode> first_graph_node() {
        ensure_nodes();
        return m_firstnode;
    }
    std::shared_ptr<GraphNode> second_graph_node() {
        ensure_nodes();
        return m_secondnode;
    }
};