#pragma once
#include "GraphNode.h"
#include <vector>
#include <memory>
#include <set>

// Determine a processing order for a set of nodes.
// Each entry in the resulting vector is a single node to process or a 
// set of nodes to co-process.
std::vector<std::set<GraphNode*>> graph_processing_order(std::set<GraphNode*> nodes);