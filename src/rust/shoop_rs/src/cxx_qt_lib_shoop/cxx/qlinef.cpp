#include "cxx-qt-lib-shoop/qlinef.h"

void clear(QList<QLineF> &list) {
    list.clear();
}

QList<QLineF> clone(QList<QLineF> const& list) {
    QList<QLineF> rval = list;
    return list;
}

bool contains(QList<QLineF> const& list, QLineF const& line) {
    return list.contains(line);
}

QList<QLineF> create() {
    return QList<QLineF>{};
}

void drop(QList<QLineF> &list) {
    list.~QList();
}