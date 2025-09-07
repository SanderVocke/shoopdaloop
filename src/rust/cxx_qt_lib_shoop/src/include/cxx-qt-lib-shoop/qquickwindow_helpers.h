#pragma once
#include <stdexcept>
#include <QQuickWindow>
#include <QObject>

inline QString qobject_as_qquickwindow_title(QObject *as_window) {
    QQuickWindow* window = qobject_cast<QQuickWindow*>(as_window);
    if (!window) {
        throw std::runtime_error("QObject is not a window");
    }
    return window->title();
}

inline bool qobject_as_qquickwindow_grab_and_save(QObject *as_window, QString filename) {
    QQuickWindow* window = qobject_cast<QQuickWindow*>(as_window);
    if (!window) {
        throw std::runtime_error("QObject is not a window");
    }
    return window->grabWindow().save(filename);
}