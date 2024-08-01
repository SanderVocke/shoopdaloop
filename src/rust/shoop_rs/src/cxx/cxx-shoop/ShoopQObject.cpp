#include "ShoopQObject.h"

ShoopQObject::ShoopQObject(QObject *parent) : QObject(parent) {}

bool ShoopQObject::amIOnObjectThread() const {
    return thread() == QThread::currentThread();
}