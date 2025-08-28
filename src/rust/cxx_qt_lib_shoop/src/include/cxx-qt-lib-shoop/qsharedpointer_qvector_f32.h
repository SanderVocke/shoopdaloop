#pragma once
#include <QSharedPointer>
#include <QVector>
#include <memory>

using QSharedPointer_QVector_f32 = QSharedPointer<QVector<float>>;

inline std::unique_ptr<QSharedPointer_QVector_f32> qSharedPointerQVectorf32FromPtr(QVector<float> *ptr) {
    return std::make_unique<QSharedPointer_QVector_f32>(ptr);
}

inline std::unique_ptr<QSharedPointer_QVector_f32> qSharedPointerCopy(QSharedPointer_QVector_f32 const& other) {
    return std::make_unique<QSharedPointer_QVector_f32>(other);
}

inline QVector<float> *qSharedPointerData(QSharedPointer_QVector_f32 const& ptr) {
    return ptr.data();
}