// Copyright 2007-2020 David Robillard <d@drobilla.net>
// SPDX-License-Identifier: ISC

#undef NDEBUG

#include "../src/lilv_internal.h"

#ifdef _WIN32
#  include <windows.h>
#  define setenv(n, v, r) SetEnvironmentVariable((n), (v))
#  define unsetenv(n) SetEnvironmentVariable((n), NULL)
#endif

#include <assert.h>
#include <stdlib.h>
#include <string.h>

int
main(void)
{
#ifndef _WIN32
  char* s = NULL;

  const char* const home = getenv("HOME");

  setenv("LILV_TEST_1", "test", 1);

  assert(!strcmp((s = lilv_expand("$LILV_TEST_1")), "test"));
  free(s);
  if (home) {
    assert(!strcmp((s = lilv_expand("~")), home));
    free(s);
    assert(!strcmp((s = lilv_expand("~foo")), "~foo"));
    free(s);

    char* const home_foo = lilv_strjoin(home, "/foo", NULL);
    assert(!strcmp((s = lilv_expand("~/foo")), home_foo));
    free(s);
    free(home_foo);
  }

  assert(!strcmp((s = lilv_expand("$NOT_A_VAR")), "$NOT_A_VAR"));
  free(s);

  unsetenv("LILV_TEST_1");
#endif

  return 0;
}
