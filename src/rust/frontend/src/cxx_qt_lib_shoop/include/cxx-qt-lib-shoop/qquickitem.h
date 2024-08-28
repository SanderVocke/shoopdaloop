#pragma once
#include <QQuickItem>
#include <QObject>

template<typename T>
inline QQuickItem* qquickitemFromPtr(T *item) {
    return static_cast<QQuickItem*>(item);
}

template<typename T>
inline QQuickItem const& qquickitemFromRef(T const& item)  {
    return static_cast<QQuickItem const&>(item);
}

inline QObject * qquickitemToQobjectPtr(QQuickItem* item) {
    return static_cast<QObject*>(item);
}

inline QObject const& qquickitemToQobjectRef(QQuickItem const& item) {
    return static_cast<QObject const&>(item);
}

inline QQuickItem * qquickitemParentItem(QQuickItem const& item) {
    return item.parentItem();
}

inline void qquickitemSetParentItem(QQuickItem *item, QQuickItem *parent) {
    item->setParentItem(parent);
}