#include "shoop_accelerate.h"
#include <chrono>
#include <cmath>
#include <cstring>
#include <algorithm>

extern "C" {

pyramid_data_t * create_audio_power_pyramid(unsigned int n_samples, float *input_data, unsigned int n_levels) {

    auto t1 = std::chrono::high_resolution_clock::now();

    auto allocate = [&](uint32_t size) {
        return (float*) malloc(sizeof(float) * size);
    };

    // Convert to power
    float* power_data = allocate(n_samples);
    for(size_t i=0; i<n_samples; i++) {
        auto &v = power_data[i];
        v = input_data[i];
        // Calculate power in dB scale
        v = std::abs(v);
        v = std::max(v, std::pow(10.0f, -45.0f));
        v = 20.0f * std::log10(v);
        // Put in 0-1 range for rendering
        v = std::max(1.0 - (-v) / 45.0f, 0.0);
    }

    auto rval = new pyramid_data_t;
    rval->n_levels = n_levels;
    rval->levels = new pyramid_data_level_t[n_levels];

    unsigned current_size = n_samples;
    for(size_t i=0; i<n_levels; i++) {
        if (i > 0 && current_size > 1) {
            // Reduce by 2
            auto prev_power_data = power_data;
            current_size /= 2;
            if (current_size > 0) {
                power_data = allocate(current_size);
                for(size_t i=0; i<current_size; i++) {
                    power_data[i] = (prev_power_data[2*i] + prev_power_data[2*i+1]) / 2.0f;
                }
            }
        }
        rval->levels[i].n_samples = current_size;
        rval->levels[i].subsampling_factor = current_size > 0 && n_samples > 0 ? n_samples / current_size : 1;
        rval->levels[i].data = allocate(current_size);
        memcpy((void*)rval->levels[i].data, (void*)power_data, sizeof(float) * current_size); 
    }

    auto t2 = std::chrono::high_resolution_clock::now();

    //std::cout << "Preprocess cost: " << std::chrono::duration_cast<std::chrono::milliseconds>(t2-t1).count() << "ms.\n";

    return rval;
}

void destroy_audio_power_pyramid(pyramid_data_t *pyramid_data) {
    for(size_t i=0; i<pyramid_data->n_levels; i++) {
        free((void*)pyramid_data->levels[i].data);
    }
    delete[] pyramid_data->levels;
    delete pyramid_data;
}

}