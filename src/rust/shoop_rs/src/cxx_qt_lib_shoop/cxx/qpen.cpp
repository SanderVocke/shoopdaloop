#include "cxx-qt-lib-shoop/qpen.h"

QPen qpen_default() {
    return QPen();
}

QPen qpen_from_color(QColor const& color) {
    return QPen(color);
}