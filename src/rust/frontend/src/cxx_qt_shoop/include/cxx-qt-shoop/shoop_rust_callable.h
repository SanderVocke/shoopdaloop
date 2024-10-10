#pragma once

#include <QVariant>
#include <QMetaType>
#include <rust/cxx.h>

struct ShoopRustCallable
{
    ::rust::Fn<QVariant(::rust::Slice<const QVariant>)> call;
};
static_assert(
    ::rust::IsRelocatable<::ShoopRustCallable>::value,
    "ShoopRustCallable must be relocatable");

Q_DECLARE_METATYPE(ShoopRustCallable);

bool
qvariantCanConvertShoopRustCallable(QVariant const& variant);

void
registerMetatypeShoopRustCallable(::rust::String & name);