#pragma once
#include <QObject>
#include <QString>
#include <QIcon>
#include <QtQuick/QQuickWindow>

template<typename App>
inline void setApplicationWindowIconPath(App &app, QString path) {
    auto icon = QIcon(path);
    app.setWindowIcon(icon);
}

inline void setWindowIconPathIfWindow(QObject *object, QString path) {
    QQuickWindow *window = qobject_cast<QQuickWindow *>(object);
    std::cout << "WINDOW BEFORE: " << window->icon().isNull() << " " << window->icon().name().toStdString() << std::endl;

    if(window) {
        std::cout << "SETTING WINDOW" << std::endl;
        auto icon = QIcon(path);
        window->setIcon(icon);
    } else {
        std::cout << "CANNOT SET WINDOW" << std::endl;
    }

    std::cout << "WINDOW AFTER: " << window->icon().isNull() << " " << window->icon().name().toStdString() << std::endl;
}