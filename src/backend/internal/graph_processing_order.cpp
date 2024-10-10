#include "graph_processing_order.h"
#include <map>
#include <stdexcept>
#include <algorithm>
#include "LoggingBackend.h"

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
            logging::log<"Backend.ProcessGraph", log_level_debug_trace>(
                std::nullopt, std::nullopt, "created annotated node for {} with {} other co-processed nodes",
                n->graph_node_name(), annotated->m_nodes.size() - 1
            );
            all_ns.insert(std::move(annotated));
        }
    }

    // Populate the annotated nodes with graph dependencies.
    for(auto &n : all_ns) {
        auto p_n = n.get();
        for(auto &nn : p_n->m_nodes) {
            for(auto d : nn->graph_node_outgoing_edges()) {
                if(auto locked = d.lock()) {
                    auto maybe_p_other = node_to_annotated_node.find(locked.get());
                    if (maybe_p_other != node_to_annotated_node.end()) {
                        auto p_other = maybe_p_other->second;
                        p_n->out.insert(p_other);
                        p_other->in.insert(p_n);
                        logging::log<"Backend.ProcessGraph", log_level_debug_trace>(
                            std::nullopt, std::nullopt, "add edge {} -> {}",
                            (*p_n->m_nodes.begin())->graph_node_name(),
                            (*p_other->m_nodes.begin())->graph_node_name()
                        );
                    }
                }
            }
            for(auto d : nn->graph_node_incoming_edges()) {
                if(auto locked = d.lock()) {
                    auto maybe_p_other = node_to_annotated_node.find(locked.get());
                    if (maybe_p_other != node_to_annotated_node.end()) {
                        auto p_other = maybe_p_other->second;
                        p_n->in.insert(p_other);
                        p_other->out.insert(p_n);
                        logging::log<"Backend.ProcessGraph", log_level_debug_trace>(
                            std::nullopt, std::nullopt, "add edge {} -> {}",
                            (*p_other->m_nodes.begin())->graph_node_name(),
                            (*p_n->m_nodes.begin())->graph_node_name()
                        );
                    }
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
        // We sort them by name too, but we need to store the names beforehand.
        std::vector<std::pair<AnnotatedGraphNode*, std::string>> nodes_to_schedule;
        nodes_to_schedule.reserve(no_incoming.size());
        auto compare_names = [](std::string a, std::string b) {
            return a < b;
        };
        auto reduce_to_name = [compare_names](std::set<GraphNode*>& s) {
            auto &node = *(*s.begin());
            auto result = node.graph_node_name();
            for(auto &g: s) {
                if (compare_names(g->graph_node_name(), result)) {
                    result = g->graph_node_name();
                }
            }
            return result;
        };
        for(auto &n : no_incoming) {
            nodes_to_schedule.push_back(std::make_pair(n, reduce_to_name(n->m_nodes)));
        }
        
        std::sort(nodes_to_schedule.begin(), nodes_to_schedule.end(), [](auto &a, auto &b) {return a.second < b.second;});

        for(auto &n: nodes_to_schedule) {
            scheduled.push_back(n.first);
            unscheduled.erase(n.first);
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