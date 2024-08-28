#pragma once

#include <QtCore/QThread>
#include <QtCore/QObject>

class ShoopQObject : public QObject {
    Q_OBJECT
  public:
    virtual ~ShoopQObject() = default;
    explicit ShoopQObject(QObject *parent = nullptr);

    bool amIOnObjectThread() const;
};

static_assert(::std::is_base_of<QObject, ShoopQObject>::value,
              "ShoopQObject must inherit from QObject");

Q_DECLARE_METATYPE(ShoopQObject *)
