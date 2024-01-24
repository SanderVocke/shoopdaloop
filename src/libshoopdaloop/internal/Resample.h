#pragma once

float* resample_single(float* in, unsigned n_frames, unsigned target_n_frames);
float* resample_multi(float* in, unsigned n_channels_outer, unsigned n_frames_inner, unsigned target_n_frames);