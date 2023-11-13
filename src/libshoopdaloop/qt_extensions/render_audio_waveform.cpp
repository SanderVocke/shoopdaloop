#include "render_audio_waveform.h"
#include <QPainter>
#include <QPen>
#include <QLine>
#include <chrono>
#include <cmath>
#include <iostream>
#include <map>
#include <vector>

struct PreprocessingData {
    std::vector<QLine> render_lines;
    std::map<uint32_t, std::vector<float>> subsampling_pyramid;
};

extern "C" {

void render_audio_waveform(void *qPainter,
                           audio_waveform_preprocessing_data_t *preprocessing_data,
                           unsigned width,
                           unsigned height,
                           unsigned samples_per_bin,
                           unsigned samples_offset) {
    auto &painter = *((QPainter*)qPainter);
    auto &preproc = *((PreprocessingData*)preprocessing_data);

    auto t1 = std::chrono::high_resolution_clock::now();

    auto n_lines = (uint32_t)std::ceil(
        std::max(0.0, std::min(8192.0, (double)width))
    );
    if (preproc.render_lines.size() != n_lines) {
        preproc.render_lines.resize(n_lines);
    }

    // Calculate lines to display
    // TODO: interpolate instead of nearest sample
    // First, select the closest subsampling level
    int subsampled_by = -1;
    for(auto const& pair : preproc.subsampling_pyramid) {
        if (pair.first <= samples_per_bin) { subsampled_by = pair.first; }
    }
    bool draw_lines = false;

    // Create the lines to render
    if (subsampled_by > 0) {
        draw_lines = true;
        auto const& samples = preproc.subsampling_pyramid[subsampled_by];
        for(uint32_t idx=0; idx < preproc.render_lines.size(); idx++) {
            float sample_idx = ((float)idx + ((float)samples_offset)/((float)samples_per_bin)) * (float)samples_per_bin / (float) subsampled_by;
            uint32_t nearest_idx = std::min(std::max(0, (int)std::round(sample_idx)), (int)samples.size());
            float sample = samples.size() > nearest_idx ? samples[nearest_idx] : 0.0f;
            if (sample_idx < 0.0f || sample_idx > (float)samples.size()) {
                // Out of range, render no audio
                preproc.render_lines[idx].setLine(
                    idx, (int)((0.5f)*height),
                    idx, (int)((0.5f)*height)
                );
            } else if (true || subsampled_by >= 4) {
                // Subsampled by such an amount that we do "abs display"
                preproc.render_lines[idx].setLine(
                    idx, (int)((0.5f - sample/2.0)*height),
                    idx, (int)((0.5f + sample/2.0)*height)
                );
            } else {
                // TODO Zoomed in enough to look at transient
            }
        }
    }

    // Draw the waveform
    if (draw_lines) {
        painter.setPen(QPen("red"));
        painter.drawLines(preproc.render_lines.data(), preproc.render_lines.size());
    }

    // Horizontal line
    painter.drawLine(0, (int)(height / 2.0f), (int)width, (int)(height / 2.0f));

    auto t2 = std::chrono::high_resolution_clock::now();

    //std::cout << "Paint cost: " << std::chrono::duration_cast<std::chrono::milliseconds>(t2-t1).count() << "ms.\n";
}

void update_preprocessing_data(audio_waveform_preprocessing_data_t *in_out_preprocessing_data,
                                            unsigned int n_samples,
                                            float *input_data)
{
    auto t1 = std::chrono::high_resolution_clock::now();

    auto &preproc = *((PreprocessingData*)in_out_preprocessing_data);

    std::vector<float> power_data(n_samples);
    for(size_t i=0; i<n_samples; i++) {
        auto &v = power_data.at(i);
        v = input_data[i];
        // Calculate power in dB scale
        v = std::abs(v);
        v = std::max(v, std::pow(10.0f, -45.0f));
        v = 20.0f * std::log10(v);
        // Put in 0-1 range for rendering
        v = std::max(1.0 - (-v) / 45.0f, 0.0);
    }

    // Subsample by 2.
    auto reduce = [&]() {
        for(uint32_t idx=0; idx<power_data.size()/2; idx++) {
            power_data[idx] = (power_data[2*idx] + power_data[2*idx+1]) / 2.0f;
        }
        power_data.resize(power_data.size() / 2);
    };

    for(uint32_t subsampled_by = 1; subsampled_by <= 2048; subsampled_by *= 2) {
        preproc.subsampling_pyramid[subsampled_by] = power_data;
        reduce();
    }

    // update();

    auto t2 = std::chrono::high_resolution_clock::now();

    //std::cout << "Preprocess cost: " << std::chrono::duration_cast<std::chrono::milliseconds>(t2-t1).count() << "ms.\n";
}

}