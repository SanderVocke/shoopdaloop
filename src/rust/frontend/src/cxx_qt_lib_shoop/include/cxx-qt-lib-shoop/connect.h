#pragma once
#include <QObject>
#include <QMetaObject>
#include <QMetaMethod>
#include <rust/cxx.h>
#include <string>
#include <exception>
#include <cxx-qt-lib-shoop/reporting.h>

template<typename A, typename B>
void connect(A const* sender,
             ::rust::String signal,
             B const* receiver,
             ::rust::String slot,
             unsigned connection_type)
{
    auto qobj_sender = static_cast<QObject const*>(sender);
    auto qobj_receiver = static_cast<QObject const*>(receiver);
    auto snd_meta = sender->metaObject();
    auto rcv_meta = receiver->metaObject();

    int signalIndex = snd_meta->indexOfSignal(signal.c_str());
    int slotIndex = rcv_meta->indexOfSlot(slot.c_str());

    if (signalIndex < 0) {
        report_signal_not_found(sender, signal.operator std::string());
    }
    if (slotIndex < 0) {
        report_method_not_found(sender, slot.operator std::string());
    }

    QMetaMethod signalMethod = snd_meta->method(signalIndex);
    QMetaMethod slotMethod = rcv_meta->method(slotIndex);

    if (!QObject::connect(qobj_sender, signalMethod, qobj_receiver, slotMethod, (Qt::ConnectionType) connection_type)) {
        throw std::runtime_error("Failed to connect");
    }
}