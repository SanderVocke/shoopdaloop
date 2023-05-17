#pragma once

#include "macros.h"
#include <string>

// #include <QtQuick/QQuickPaintedItem>

// class RenderAudioWaveform : public QQuickPaintedItem {

// };

class BINDINGS_API RenderAudioWaveform {
public:
    std::string give_me_a_string() const;
};