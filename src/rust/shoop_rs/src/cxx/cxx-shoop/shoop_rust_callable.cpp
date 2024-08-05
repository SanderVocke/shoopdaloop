#include "shoop_rust_callable.h"

bool
qvariantCanConvertShoopRustCallable(const QVariant& variant)
{
  return variant.canConvert<ShoopRustCallable>();
}

void
registerMetatypeShoopRustCallable(const ::rust::String & name)
{
  qRegisterMetaType<ShoopRustCallable>(name.c_str());
}