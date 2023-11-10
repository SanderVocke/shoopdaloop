extern "C" {
#include "custom_qt_msg_handler.h"
}

#include <QtCore>
#include "LoggingBackend.h"
#include <memory>

bool g_abort_on_fatal = true;

void QtMsgHandler(QtMsgType type, const QMessageLogContext &context, const QString &msg) {
    QByteArray localMsg = msg.toLocal8Bit();
    auto file = context.file ? context.file : "";
    auto function = context.function ? context.function : "";
    auto line = context.line ? context.line : 0;
    auto category = context.category ? context.category : "";
    auto _msg = localMsg.constData();

    switch (type) {
        case QtDebugMsg:
            logging::log<"Backend.QtInternal", debug>("{}:{}:{}:{}: {}", file, function, line, category, _msg);
            break;
        case QtInfoMsg:
            logging::log<"Backend.QtInternal", info>("{}:{}:{}:{}: {}", file, function, line, category, _msg);
            break;
        case QtWarningMsg:
            logging::log<"Backend.QtInternal", warning>("{}:{}:{}:{}: {}", file, function, line, category, _msg);
            break;
        case QtCriticalMsg:
            logging::log<"Backend.QtInternal", error>("{}:{}:{}:{}: {}", file, function, line, category, _msg);
            break;
        case QtFatalMsg:
            if (g_abort_on_fatal) {
                logging::log<"Backend.QtInternal", error>("FATAL {}:{}:{}:{}: {}", file, function, line, category, _msg);
                abort();
            } else {
                logging::log<"Backend.QtInternal", error>("FATAL (IGNORED) {}:{}:{}:{}: {}", file, function, line, category, _msg);
            }
            break;
    }
}

extern "C" {

void install_custom_qt_msg_handler(unsigned abort_on_fatal) {
    g_abort_on_fatal = abort_on_fatal != 0;
    qInstallMessageHandler(QtMsgHandler);
}

}