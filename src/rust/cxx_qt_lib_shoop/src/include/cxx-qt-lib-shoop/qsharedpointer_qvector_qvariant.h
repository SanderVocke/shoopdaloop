#pragma once
#include <QSharedPointer>
#include <QVector>
#include <QVariant>
#include <memory>

using QSharedPointer_QVector_QVariant = QSharedPointer<QVector<QVariant>>;

inline std::unique_ptr<QSharedPointer_QVector_QVariant> qSharedPointerQVectorQVariantFromPtr(QVector<QVariant> *ptr) {
    return std::make_unique<QSharedPointer_QVector_QVariant>(ptr);
}

inline std::unique_ptr<QSharedPointer_QVector_QVariant> qSharedPointerCopy(QSharedPointer_QVector_QVariant const& other) {
    return std::make_unique<QSharedPointer_QVector_QVariant>(other);
}

inline QVector<QVariant> *qSharedPointerData(QSharedPointer_QVector_QVariant const& ptr) {
    return ptr.data();
}