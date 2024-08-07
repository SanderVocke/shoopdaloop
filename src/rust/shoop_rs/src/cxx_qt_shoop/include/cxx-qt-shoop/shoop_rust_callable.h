#pragma once

#include <QtCore/QVariant>
#include <QtCore/QMetaType>
#include <rust/cxx.h>

struct ShoopRustCallable
{
    // ::rust::Box<::rust::Fn<void()>> callable;
    int hello;
};
Q_DECLARE_METATYPE(ShoopRustCallable);

bool
qvariantCanConvertShoopRustCallable(QVariant const& variant);

void
registerMetatypeShoopRustCallable(::rust::String & name);