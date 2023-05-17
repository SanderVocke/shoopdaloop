#include "RenderAudioWaveform.h"
#include <QPainter>
#include <chrono>
#include <cmath>
#include <iostream>

void RenderAudioWaveform::paint(QPainter *painter) {
    auto t1 = std::chrono::high_resolution_clock::now();

    // Calculate lines to display
    // TODO: interpolate instead of nearest sample
    // First, select the closest subsampling level
    int subsampled_by = -1;
    for(auto const& pair : m_subsampling_pyramid) {
        if (pair.first <= m_samples_per_bin) { subsampled_by = pair.first; }
    }
    bool draw_lines = false;

    // Create the lines to render
    if (subsampled_by > 0) {
        draw_lines = true;
        auto const& samples = m_subsampling_pyramid[subsampled_by];
        for(size_t idx=0; idx < m_render_lines.size(); idx++) {
            float sample_idx = ((float)idx + ((float)m_samples_offset)/((float)m_samples_per_bin)) * (float)m_samples_per_bin / (float) subsampled_by;
            size_t nearest_idx = std::min(std::max(0, (int)std::round(sample_idx)), (int)samples.size());
            float sample = samples[nearest_idx];
            if (true || subsampled_by >= 4) {
                // Subsampled by such an amount that we do "abs display"
                m_render_lines[idx].setLine(
                    idx, (int)((0.5f - sample/2.0)*height()),
                    idx, (int)((0.5f + sample/2.0)*height())
                );
            } else {
                // TODO Zoomed in enough to look at transient
            }
        }
    }

    // Background
    painter->fillRect(0, 0, width(), height(), QColor("black"));

    // Foreground
    if (draw_lines) {
        painter->setPen(QPen("red"));
        painter->drawLines(m_render_lines.data(), m_render_lines.size());
    }

    // Horizontal line
    painter->drawLine(0, (int)(height() / 2.0f), (int)width(), (int)(height() / 2.0f));

    auto t2 = std::chrono::high_resolution_clock::now();

    //std::cout << "Paint cost: " << std::chrono::duration_cast<std::chrono::milliseconds>(t2-t1).count() << "ms.\n";
}

RenderAudioWaveform::RenderAudioWaveform(QQuickItem *parent)
    : QQuickPaintedItem(parent) {
    QObject::connect(this, &RenderAudioWaveform::inputDataChanged,
                     this, &RenderAudioWaveform::preprocess);
    QObject::connect(this, &RenderAudioWaveform::widthChanged,
                     this, &RenderAudioWaveform::update_lines);
    QObject::connect(this, &RenderAudioWaveform::samplesOffsetChanged,
                     this, [this]() { update(); });
    QObject::connect(this, &RenderAudioWaveform::samplesPerBinChanged,
                     this, [this]() { update(); });
}

RenderAudioWaveform::~RenderAudioWaveform() {}

void RenderAudioWaveform::update_lines() {
    m_render_lines.resize((size_t)std::ceil(width()));
}

void RenderAudioWaveform::preprocess() {
    auto t1 = std::chrono::high_resolution_clock::now();

    if (!m_input_data.canConvert<QList<QVariant>>()) {
        m_subsampling_pyramid.clear();
        return;
    }
    auto maybe_floats = m_input_data.value<QList<QVariant>>();
    for (auto const& v: maybe_floats) {
        if(!v.canConvert<float>()) {
            m_subsampling_pyramid.clear();
            return;
        }
    }

    std::vector<float> data(maybe_floats.size());

    size_t idx=0;
    for(auto const& v : maybe_floats) {
        data[idx++] = v.value<float>();
    }

    for(auto &v: data) {
        // Calculate power in dB scale
        v = std::abs(v);
        v = std::max(v, std::pow(10.0f, -45.0f));
        v = 20.0f * std::log10(v);
        // Put in 0-1 range for rendering
        v = std::max(1.0 - (-v) / 45.0f, 0.0);
    }

    // Subsample by 2.
    auto reduce = [&]() {
        for(size_t idx=0; idx<data.size()/2; idx++) {
            data[idx] = (data[2*idx] + data[2*idx+1]) / 2.0f;
        }
        data.resize(data.size() / 2);
    };

    for(size_t subsampled_by = 1; subsampled_by <= 2048; subsampled_by *= 2) {
        m_subsampling_pyramid[subsampled_by] = data;
        reduce();
    }

    auto t2 = std::chrono::high_resolution_clock::now();

    //std::cout << "Preprocess cost: " << std::chrono::duration_cast<std::chrono::milliseconds>(t2-t1).count() << "ms.\n";
}