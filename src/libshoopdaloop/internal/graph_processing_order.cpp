#include "graph_processing_order.h"
#include <map>

// Represents one or more nodes (more means they should be co-processed together).
// Annotated redundantly with both incoming and outgoing edges.
struct AnnotatedGraphNode {
    std::set<GraphNode *> m_nodes;
    std::set<AnnotatedGraphNode*> in;
    std::set<AnnotatedGraphNode*> out;
};

std::vector<std::set<GraphNode*>> graph_processing_order(std::set<GraphNode *> nodes) {
    // Implementation is a variation on Kahn's algorithm (https://en.wikipedia.org/wiki/Topological_sorting)
    
    // Create annotated nodes that will allow us to store more information.
    std::set<std::unique_ptr<AnnotatedGraphNode>> all_ns;
    std::map<GraphNode*, AnnotatedGraphNode*> node_to_annotated_node;
    for(auto n : nodes) {
        auto maybe_found = node_to_annotated_node.find(n);
        if(maybe_found != node_to_annotated_node.end()) {
            maybe_found->second->m_nodes.insert(n);
        } else {
            auto annotated = std::make_unique<AnnotatedGraphNode>();
            annotated->m_nodes.insert(n);
            for (auto &node : n->graph_node_co_process_nodes()) {
                if (auto _node = node.lock()) {
                    annotated->m_nodes.insert(_node.get());
                    node_to_annotated_node.insert(std::make_pair(_node.get(), annotated.get()));
                }
            }
            node_to_annotated_node.insert(std::make_pair(n, annotated.get()));
            all_ns.insert(std::move(annotated));
        }
    }

    // Populate the annotated nodes with graph dependencies.
    for(auto &n : all_ns) {
        auto p_n = n.get();
        for(auto &nn : p_n->m_nodes) {
            for(auto d : nn->graph_node_outgoing_edges()) {
                if(auto locked = d.lock()) {
                    auto p_other = node_to_annotated_node.at(locked.get());
                    p_n->out.insert(p_other);
                    p_other->in.insert(p_n);
                }
            }
            for(auto d : nn->graph_node_incoming_edges()) {
                if(auto locked = d.lock()) {
                    auto p_other = node_to_annotated_node.at(locked.get());
                    p_n->in.insert(p_other);
                    p_other->out.insert(p_n);
                }
            }
        }
    }

    // Create two collections: one for the nodes already scheduled and one for the rest.
    // Scheduled nodes are grouped in sets which should be co-processed together.
    std::vector<AnnotatedGraphNode*> scheduled;
    std::set<AnnotatedGraphNode*> unscheduled;
    for (auto &n: all_ns) { unscheduled.insert(n.get()); }

    // While there are unscheduled nodes, find the ones with no incoming edges and schedule them.
    while(!unscheduled.empty()) {
        // Find the nodes with no incoming edges.
        std::set<AnnotatedGraphNode*> no_incoming;
        for(auto &n : unscheduled) {
            if(n->in.empty()) {
                no_incoming.insert(n);
            }
        }

        // Remove any nodes not eligible due to co-processing constraints.
        auto no_incoming_clone = no_incoming;
        for(auto &n : no_incoming_clone) {
            if (no_incoming.find(n) == no_incoming.end()) {
                no_incoming.erase(n);
            }
        }

        // If there are no nodes with no incoming edges, we have a cycle.
        if(no_incoming.empty()) {
            throw std::runtime_error("Cycle in graph or unsolveable co-processing constraint");
        }

        // Now we have all our processing steps. TODO: order them in priority order.

        // Schedule the eligible nodes.
        std::vector<AnnotatedGraphNode*> nodes_to_schedule(no_incoming.begin(), no_incoming.end());
        // First, order the to-be-scheduled nodes by name for determinism.
        auto compare_names = [](std::string a, std::string b) {
            return a < b;
        };
        auto reduce_set = [&](std::set<GraphNode*>& s) {
            auto result = (*s.begin())->graph_node_name();
            for(auto &g: s) {
                if (compare_names(g->graph_node_name(), result)) {
                    result = g->graph_node_name();
                }
            }
            return result;
        };
        auto compare_sets = [&](std::set<GraphNode*>& a, std::set<GraphNode*>& b) {
            return reduce_set(a) < reduce_set(b);
        };
        auto compare_annotated_nodes = [&](AnnotatedGraphNode* a, AnnotatedGraphNode* b) {
            return compare_sets(a->m_nodes, b->m_nodes);
        };
        std::sort(nodes_to_schedule.begin(), nodes_to_schedule.end(), compare_annotated_nodes);

        for(auto &n: nodes_to_schedule) {
            scheduled.push_back(n);
            unscheduled.erase(n);
        }

        // Remove the scheduled nodes from the unscheduled nodes' incoming edges.
        for(auto &n : unscheduled) {
            for(auto &s : scheduled) {
                n->in.erase(s);
            }
        }
    }

    // Convert the scheduled nodes into the final result.
    std::vector<std::set<GraphNode*>> result;
    for(auto &n : scheduled) {
        result.push_back(n->m_nodes);
    }

    return result;
}