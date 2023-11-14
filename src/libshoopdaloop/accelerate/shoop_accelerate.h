#pragma once

#ifdef _WIN32
    #define SHOOP_EXPORT __declspec(dllexport)
#else
    #define SHOOP_EXPORT __attribute__((visibility("default")))
#endif

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    unsigned int n_samples;
    unsigned int subsampling_factor;
    float *data;
} pyramid_data_level_t;

typedef struct {
    unsigned int n_levels;
    pyramid_data_level_t *levels;
} pyramid_data_t;

SHOOP_EXPORT pyramid_data_t * create_audio_power_pyramid(unsigned int n_samples, float *input_data, unsigned int n_levels);
SHOOP_EXPORT void destroy_audio_power_pyramid(pyramid_data_t *pyramid_data);

#ifdef __cplusplus
}
#endif