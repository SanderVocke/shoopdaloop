#include "cxx-qt-shoop/shoop_rust_callable.h"

bool
qvariant_can_convert_shoop_rust_callable(const QVariant& variant)
{
  return variant.canConvert<ShoopRustCallable>();
}

void
register_metatype_shoop_rust_callable(::rust::String & name)
{
  qRegisterMetaType<ShoopRustCallable>(name.c_str());
}