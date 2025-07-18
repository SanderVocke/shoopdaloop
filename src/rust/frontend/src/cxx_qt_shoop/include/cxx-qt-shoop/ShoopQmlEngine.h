#pragma once
#include <QtQml/QQmlApplicationEngine>
#include <QtQuick/QQuickWindow>
#include <QObject>
#include <stdexcept>

class ShoopQmlEngine : public QQmlApplicationEngine {
    public:
    ShoopQmlEngine(QObject *parent = nullptr)
        : QQmlApplicationEngine(parent)
    {}

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
        } else {
            root->deleteLater();
        }
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
};