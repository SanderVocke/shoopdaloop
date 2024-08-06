#pragma once

#include <QtCore/QLineF>
#include <QtCore/QList>

void clear(QList<QLineF> &list);

QList<QLineF> clone(QList<QLineF> const& list);

bool contains(QList<QLineF> const& list, QLineF const& line);

QList<QLineF> create();

void drop(QList<QLineF> &list);