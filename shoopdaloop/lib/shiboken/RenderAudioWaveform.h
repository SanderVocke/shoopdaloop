#pragma once

#include "macros.h"
#include <qqmlintegration.h>
#include <qtmetamacros.h>
#include <qwindowdefs.h>
#include <string>

#include <QtQuick/QQuickPaintedItem>
#include <QVariant>

class BINDINGS_API RenderAudioWaveform : public QQuickPaintedItem {
    Q_OBJECT
    QML_ELEMENT

    QVariant m_input_data;

public:
    RenderAudioWaveform(QQuickItem *parent = nullptr);
    void paint(QPainter* painter) override;

    Q_PROPERTY(
        QVariant input_data
        MEMBER m_input_data
        NOTIFY inputDataChanged
    )

signals:
    void inputDataChanged();
};