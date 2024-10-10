#include "cxx-qt-shoop/shoop_rust_callable.h"

bool
qvariantCanConvertShoopRustCallable(const QVariant& variant)
{
  return variant.template canConvert<ShoopRustCallable>();
}

void
registerMetatypeShoopRustCallable(::rust::String & name)
{
  qRegisterMetaType<ShoopRustCallable>(name.c_str());
}