#pragma once
#include <QtQml/QQmlApplicationEngine>
#include <QtQuick/QQuickWindow>
#include <QObject>
#include <QQmlContext>
#include <stdexcept>
#include <QString>
#include <QVariant>
#include <QThread>
#include <QPointer>
#include <rust/cxx.h>
#include <iostream>
class ShoopQmlEngine : public QQmlApplicationEngine {
    Q_OBJECT

    public:
    explicit ShoopQmlEngine(QObject *parent = nullptr);

    virtual ~ShoopQmlEngine();

    void closeRoot() {
        auto root_objects = this->rootObjects();
        if(root_objects.size() > 1) {
            throw std::runtime_error("More than one root QML object found, unexpected");
        }
        if(root_objects.size() == 0) {
            return;
        }

        QObject* root = root_objects.first();

        if (!root) { return; }

        if (root->inherits("QQuickWindow")) {
            auto window = static_cast<QQuickWindow*>(root);
            window->close();
        }
        root->deleteLater();
    }

    QObject * getRootWindow() {
        auto root_objects = this->rootObjects();
        if(root_objects.size() > 1) {
            throw std::runtime_error("More than one root QML object found, unexpected");
        }
        if(root_objects.size() == 0) {
            return nullptr;
        }

        QObject* root = root_objects.first();

        if (!root) { return nullptr; }

        if (!root->inherits("QQuickWindow")) {
            throw std::runtime_error("Expected QML root object to be a QQuickWindow, but it was not.");
        }

        return root;
    }

public slots:
    void setRootContextProperty(QString const& key, QVariant const& value) {
        this->rootContext()->setContextProperty(key, value);
    }
};

// Note: not thread-safe
extern QPointer<ShoopQmlEngine> g_registered_qml_engine;

Q_DECLARE_METATYPE(ShoopQmlEngine *)

inline ShoopQmlEngine* getRegisteredQmlEngine() {
    return g_registered_qml_engine.data();
}

inline ::rust::String getQmlEngineStackTrace(ShoopQmlEngine &engine) {
    if(engine.thread() != QThread::currentThread()) {
        // Can only retrieve the stack if on the same thread.
        return ::rust::String("unknown (JS engine not on querying thread)\n");
    }

    engine.throwError(QString("Crash handler stack retrieval helper error"));
    auto err = engine.catchError();
    if (err.isUndefined()) {
        return ::rust::String("unknown (couldn't catch helper error)\n");
    }

    auto stack = err.property("stack");
    if (stack.isUndefined()) {
        return ::rust::String("unknown (stack is undefined)\n");
    }

    auto stack_str = stack.toString() + "\n";
    return ::rust::String(stack_str.toStdString());
}

inline void registerQmlEngine(ShoopQmlEngine* engine) {
    g_registered_qml_engine = engine;
}

inline void setCppOwnership(QObject * object) {
    QQmlEngine::setObjectOwnership(object, QQmlEngine::CppOwnership);
}

inline void setJavascriptOwnership(QObject *object) {
    QQmlEngine::setObjectOwnership(object, QQmlEngine::JavaScriptOwnership);
}