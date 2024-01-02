#include "graph_dot.h"
#include "GraphNode.h"
#include "fmt/format.h"
#include <map>
#include <fmt/core.h>

std::string graph_dot(std::set<GraphNode*> nodes) {
    std::string r = "digraph {\n";

    std::map<GraphNode*, std::string> node_names;
    for (auto node : nodes) {
        node_names[node] = fmt::format("{} @ {}", node->graph_node_name(), fmt::ptr(node));
    }

    for (auto node : nodes) {
        for (auto incoming : node->graph_node_incoming_edges()) {
            if(auto i = incoming.lock()) {
                r += fmt::format("  \"{}\" -> \"{}\";\n", node_names.at(i.get()), node_names[node]);
            }
        }
        for (auto outgoing : node->graph_node_outgoing_edges()) {
            if(auto o = outgoing.lock()) {
                r += fmt::format("  \"{}\" -> \"{}\";\n", node_names[node], node_names[o.get()]);
            }
        }
        // TODO: represent co-processing
    }

    r += "}\n";

    return r;
}