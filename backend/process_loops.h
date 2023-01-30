#pragma once
#include <vector>
#include <memory>
#include <optional>
#include <stdexcept>
#include <iostream>

template<typename Loop>
void process_loops(std::vector<std::shared_ptr<Loop>> const& loops, size_t n_samples) {
    size_t process_until = n_samples;

    // Gather up all POIs.
    for(size_t i=0; i<loops.size(); i++) {
        size_t poi = loops[i]->get_next_poi().value_or(n_samples);
        process_until = std::min(process_until, poi);
    }

    // Process until the first POI, then handle POIs and triggers.
    for(size_t i=0; i<loops.size(); i++) {
        loops[i]->process(process_until);
    }
    for(size_t i=0; i<loops.size(); i++) {
        loops[i]->handle_poi();
    }
    for(size_t i=0; i<loops.size(); i++) {
        loops[i]->handle_sync();
    }

    // If we didn't process the whole thing, keep going.
    if(process_until < n_samples) {
        process_loops(loops, n_samples - process_until);
    }
}