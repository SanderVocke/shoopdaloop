#pragma once

#include "macros.h"
#include <qline.h>
#include <qqmlintegration.h>
#include <qtmetamacros.h>
#include <qwindowdefs.h>
#include <string>

#include <QtQuick/QQuickPaintedItem>
#include <QVariant>
#include <vector>

#include "libshoopdaloop_cpp_objects_export.h"

class BINDINGS_API LIBSHOOPDALOOP_CPP_OBJECTS_EXPORT RenderAudioWaveform : public QQuickPaintedItem {
    Q_OBJECT
    QML_ELEMENT

    QVariant m_input_data;
    float m_samples_per_bin;
    int m_samples_offset;
    std::vector<QLine> m_render_lines;
    std::map<size_t, std::vector<float>> m_subsampling_pyramid;

public:
    explicit RenderAudioWaveform(QQuickItem *parent = nullptr);
    ~RenderAudioWaveform() override;
    void paint(QPainter* painter) override;

    Q_PROPERTY(
        QVariant input_data
        MEMBER m_input_data
        NOTIFY inputDataChanged
    )

    Q_PROPERTY(
        float samples_per_bin
        MEMBER m_samples_per_bin
        NOTIFY samplesPerBinChanged
    )

    Q_PROPERTY(
        int samples_offset
        MEMBER m_samples_offset
        NOTIFY samplesOffsetChanged
    )

public slots:
    void preprocess();
    void update_lines();

signals:
    void inputDataChanged();
    void samplesPerBinChanged();
    void samplesOffsetChanged();
};