#include "Resample.h"
#include "LoggingBackend.h"

#include <cstdlib>
#include <cstring>
#include <algorithm>
#include <cstdint>
#include <optional>

#ifdef _WIN32
#undef min
#undef max
#endif

#ifdef _WIN32
#define HAVE_STRUCT_TIMESPEC
#endif
#include <zita-resampler/vresampler.h>

#ifdef _WIN32
#undef min
#undef max
#endif

float* resample_multi(float* in, unsigned n_channels_inner, unsigned n_frames_outer, unsigned target_n_frames) {
    if (target_n_frames == 0 || n_channels_inner == 0) { return nullptr; }

    auto rval = (float*)malloc(sizeof(float) * target_n_frames * n_channels_inner);

    if(n_frames_outer == 0) {
        memset((void*)rval, 0, sizeof(float) * target_n_frames);
        return rval;
    }
    
    VResampler resampler;
    double ratio = std::min(std::max(
        (double)target_n_frames / (double)n_frames_outer,
        1.0 / 16.0),
        64.0
    );
    resampler.setup(ratio, n_channels_inner, 48);

    // insert zero values at the start
    uint32_t n_zeros = resampler.inpsize() / 2 - 1;
    resampler.inp_count = n_zeros;
    resampler.inp_data = 0;
    resampler.out_count = target_n_frames;
    resampler.out_data = 0;
    while (resampler.inp_count > 0) { resampler.process(); }

    unsigned dist = resampler.inpdist();
    if(dist != 0) {
        // Should never happen - program error
        logging::log<"Backend.Resample", log_level_error>(std::nullopt, std::nullopt, "Resampler input distance is not zero: {}", dist); 
    }

    // Final pass
    resampler.out_count = target_n_frames;
    resampler.inp_count = n_frames_outer;
    resampler.inp_data = in;
    resampler.out_data = rval;

    while (resampler.inp_count > 0 && resampler.out_count > 0) {
        resampler.process();
    }
    
    // If any output samples were not filled, just copy the most recent sample
    for(size_t i=0; i<std::min(target_n_frames, resampler.out_count); i++) {
        auto *last = rval + (n_channels_inner * target_n_frames) - n_channels_inner;
        auto *last_valid = last - resampler.out_count*n_channels_inner;
        auto *target = last - i*n_channels_inner;
        for (size_t c=0; c<n_channels_inner; c++) {
            target[c] = last_valid[c];
        }
    }

    return rval;
}