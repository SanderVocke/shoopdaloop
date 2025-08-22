#include "cxx-qt-shoop/ShoopQmlEngine.h"

QPointer<ShoopQmlEngine> g_registered_qml_engine;

ShoopQmlEngine::~ShoopQmlEngine() {}

ShoopQmlEngine::ShoopQmlEngine(QObject *parent)
        : QQmlApplicationEngine(parent)
    {}