#pragma once

#ifdef _WIN32
    #define SHOOP_EXPORT __declspec(dllexport)
#else
    #define SHOOP_EXPORT __attribute__((visibility("default")))
#endif

#ifdef __cplusplus
extern "C" {
#endif

typedef struct _audio_waveform_preprocessing_data audio_waveform_preprocessing_data_t;

SHOOP_EXPORT audio_waveform_preprocessing_data_t * create_audio_waveform_preprocessing_data();
SHOOP_EXPORT void destroy_audio_waveform_preprocessing_data(audio_waveform_preprocessing_data_t *preprocessing_data);
SHOOP_EXPORT void render_audio_waveform(void *qPainter,
                                              audio_waveform_preprocessing_data_t *preprocessing_data,
                                              unsigned width,
                                              unsigned height,
                                              unsigned samples_per_bin,
                                              unsigned samples_offset);
SHOOP_EXPORT void update_preprocessing_data(audio_waveform_preprocessing_data_t *in_out_preprocessing_data,
                                            unsigned int n_samples,
                                            float *input_data);

#ifdef __cplusplus
}
#endif