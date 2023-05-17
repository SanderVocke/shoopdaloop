#include "RenderAudioWaveform.h"
#include <QPainter>
#include <chrono>
#include <cmath>
#include <iostream>

void RenderAudioWaveform::paint(QPainter *painter) {
    // Background
    painter->fillRect(0, 0, width(), height(), QColor("black"));

    // Foreground
    painter->setPen(QPen("red"));
    painter->drawLines(m_lines.data(), std::min(m_lines.size(), (size_t)width()));

    // Horizontal line
    painter->drawLine(0, (int)(height() / 2.0f), (int)width(), (int)(height() / 2.0f));
}

RenderAudioWaveform::RenderAudioWaveform(QQuickItem *parent)
    : QQuickPaintedItem(parent) {
    QObject::connect(this, &RenderAudioWaveform::inputDataChanged,
                     this, &RenderAudioWaveform::update_lines);
}

RenderAudioWaveform::~RenderAudioWaveform() {}

void RenderAudioWaveform::update_lines() {
    auto t1 = std::chrono::high_resolution_clock::now();

    if (!m_input_data.canConvert<QList<QVariant>>()) {
        m_lines.clear();
        return;
    }
    auto maybe_floats = m_input_data.value<QList<QVariant>>();
    for (auto const& v: maybe_floats) {
        if(!v.canConvert<float>()) {
            m_lines.clear();
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

    // TODO reduce
    m_lines = std::vector<QLine>(data.size());
    idx=0;
    for(auto const& v: data) {
        m_lines[idx++] = QLine(
            idx, (int)((0.5f - v/2.0)*height()),
            idx, (int)((0.5f + v/2.0)*height())
        );
    }

    auto t2 = std::chrono::high_resolution_clock::now();

    std::cout << "Update lines cost: " << std::chrono::duration_cast<std::chrono::milliseconds>(t2-t1).count() << "ms.\n";
}