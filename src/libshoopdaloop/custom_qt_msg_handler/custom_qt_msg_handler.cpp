extern "C" {
#include "custom_qt_msg_handler.h"
}

#include <QtCore>
#include "LoggingBackend.h"
#include <memory>

bool g_abort_on_fatal = true;
logging::logger *g_qt_msg_logger;

void QtMsgHandler(QtMsgType type, const QMessageLogContext &context, const QString &msg) {
    QByteArray localMsg = msg.toLocal8Bit();
    auto file = context.file ? context.file : "";
    auto function = context.function ? context.function : "";
    auto line = context.line ? context.line : 0;
    auto category = context.category ? context.category : "";

    auto formatted = fmt::format(fmt::runtime("{}:{}:{}:{}: {}"), file, function, line, category, localMsg.constData());

    switch (type) {
        case QtDebugMsg:
            g_qt_msg_logger->debug("{}", formatted);
            break;
        case QtInfoMsg:
            g_qt_msg_logger->info("{}", formatted);
            break;
        case QtWarningMsg:
            g_qt_msg_logger->warn("{}", formatted);
            break;
        case QtCriticalMsg:
            g_qt_msg_logger->error("{}", formatted);
            break;
        case QtFatalMsg:
            if (g_abort_on_fatal) {
                g_qt_msg_logger->error("FATAL: {}", formatted);
                abort();
            } else {
                g_qt_msg_logger->error("FATAL (IGNORED): {}", formatted);
            }
            break;
    }
}

extern "C" {

void install_custom_qt_msg_handler(unsigned abort_on_fatal) {
    g_abort_on_fatal = abort_on_fatal != 0;
    g_qt_msg_logger = &logging::get_logger("Backend.QtInternal");
    qInstallMessageHandler(QtMsgHandler);
}

}