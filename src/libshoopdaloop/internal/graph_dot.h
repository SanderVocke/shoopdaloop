#pragma once
#include "GraphNode.h"
#include <set>
#include <string>

std::string graph_dot(std::set<GraphNode*> nodes);