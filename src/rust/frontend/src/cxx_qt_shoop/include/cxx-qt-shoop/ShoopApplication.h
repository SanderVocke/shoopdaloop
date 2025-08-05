#pragma once
#include <QtGui/QGuiApplication>
#include <QtQuick/QQuickWindow>
#include <QtGui/QIcon>

inline int &dummy_argc() {
    static int argc = 1;
    return argc;
}

inline char **dummy_argv() {
    static const char* app_name = "ShoopDaLoop";
    return (char**) &app_name;
}

class ShoopApplication : public QGuiApplication {
    public:
    ShoopApplication(QObject *parent = nullptr) : QGuiApplication(dummy_argc(), dummy_argv()) { (void) parent; }
};

template<typename App>
inline void setWindowIconPath(App &app, QString const& path) {
    auto icon = QIcon(path);
    app.setWindowIcon(icon);
}

inline void setWindowIconPathIfWindow(QObject *object, QString const& path) {
    QQuickWindow *window = qobject_cast<QQuickWindow *>(object);
    if(window) {
        std::cout << "MATCH" << std::endl;
        auto icon = QIcon(path);
        window->setIcon(icon);
    } else {
        std::cout << "WOMP" << std::endl;
    }
}