#pragma once

#include <QPixmap>

namespace material
{
	QPixmap pixmap(const QString& name, const QSize& size = QSize(16, 16), const QColor& color = Qt::black);
}
