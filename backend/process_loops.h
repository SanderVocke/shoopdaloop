#pragma once
#include "LoopInterface.h"
#include <memory>
#include <optional>
#include <vector>

void process_loops(std::vector<std::shared_ptr<LoopInterface>> loops, size_t n_samples) {
    size_t processed = 0;

    // Repeatedly gather the next POIs of all loops, then process them all to the
    // first POI, until we have processed n_samples.
    while(processed < n_samples) {
        size_t process_until = n_samples - processed;
        for(auto &loop : loops) {
            auto poi = loop->get_next_poi();
            if (poi) { process_until = std::min(process_until, poi); }
        }
        for(auto &loop : loops) {
            loop->process(process_until);
            loop->handle_poi();
        }
        processed += process_until;
    }
}