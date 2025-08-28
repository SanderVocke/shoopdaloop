#pragma once

#include <QtCore/QObject>
#include <QtCore/QCoreApplication>
#include <QEvent>
#include <QKeyEvent>


class ShoopKeyEventParser : public QObject {
    Q_OBJECT
  public:
    virtual ~ShoopKeyEventParser() = default;
    explicit ShoopKeyEventParser(QObject *parent = nullptr);

    QObject * installed_to = nullptr;

    bool eventFilter(QObject *obj, QEvent *event) override;

    virtual void on_shift_pressed() {}
    virtual void on_shift_released() {}
    virtual void on_control_pressed() {}
    virtual void on_control_released() {}
    virtual void on_alt_pressed() {}
    virtual void on_alt_released() {}


public slots:
    void install();
};

static_assert(::std::is_base_of<QObject, ShoopKeyEventParser>::value,
              "ShoopKeyEventParser must inherit from QObject");

Q_DECLARE_METATYPE(ShoopKeyEventParser *)
