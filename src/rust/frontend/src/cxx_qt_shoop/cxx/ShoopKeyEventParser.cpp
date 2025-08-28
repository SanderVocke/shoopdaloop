#include "cxx-qt-shoop/ShoopKeyEventParser.h"


ShoopKeyEventParser::ShoopKeyEventParser(QObject* parent) : QObject(parent) {}

bool ShoopKeyEventParser::eventFilter(QObject *obj, QEvent *event)
{
    if (event->type() == QEvent::KeyPress || event->type() == QEvent::KeyRelease) {
        QKeyEvent *keyEvent = static_cast<QKeyEvent *>(event);
        if(keyEvent) {
            if(keyEvent->type() == QEvent::KeyPress && keyEvent->key() == Qt::Key_Shift) {
                on_shift_pressed();
            }
            if(keyEvent->type() == QEvent::KeyRelease && keyEvent->key() == Qt::Key_Shift) {
                on_shift_released();
            }
            if(keyEvent->type() == QEvent::KeyPress && keyEvent->key() == Qt::Key_Control) {
                on_control_pressed();
            }
            if(keyEvent->type() == QEvent::KeyRelease && keyEvent->key() == Qt::Key_Control) {
                on_control_released();
            }
            if(keyEvent->type() == QEvent::KeyPress && keyEvent->key() == Qt::Key_Alt) {
                on_alt_pressed();
            }
            if(keyEvent->type() == QEvent::KeyRelease && keyEvent->key() == Qt::Key_Alt) {
                on_alt_released();
            }
        }
    }
    return QObject::eventFilter(obj, event);
}


void ShoopKeyEventParser::install() {
    if (!installed_to) {
        auto app = static_cast<QObject*>(QCoreApplication::instance());
        app->installEventFilter(this);
        installed_to = app;
    }
}