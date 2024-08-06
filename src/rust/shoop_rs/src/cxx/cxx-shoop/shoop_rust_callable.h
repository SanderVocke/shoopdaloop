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
qvariant_can_convert_shoop_rust_callable(QVariant const& variant);

void
register_metatype_shoop_rust_callable(::rust::String & name);