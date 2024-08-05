#pragma once

#include <QtCore/QVariant>
#include <QtCore/QMetaType>
#include <rust/cxx.h>

struct ShoopRustCallable
{
    ::rust::Box<::rust::Fn<void()>> callable;
};
Q_DECLARE_METATYPE(ShoopRustCallable);

bool
qvariantCanConvertShoopRustCallable(const QVariant& variant);

void
registerMetatypeShoopRustCallable(const ::rust::String & name);