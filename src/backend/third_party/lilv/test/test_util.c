// Copyright 2007-2022 David Robillard <d@drobilla.net>
// SPDX-License-Identifier: ISC

#undef NDEBUG

#include "../src/filesystem.h"

#include "lilv/lilv.h"

#include <assert.h>
#include <stdio.h>

int
main(void)
{
  assert(!lilv_path_canonical(NULL));

  char* const dir    = lilv_create_temporary_directory("lilv_test_util_XXXXXX");
  char* const a_path = lilv_path_join(dir, "copy_a_XXXXXX");
  char* const b_path = lilv_path_join(dir, "copy_b_XXXXXX");

  FILE* const fa = fopen(a_path, "w");
  FILE* const fb = fopen(b_path, "w");
  fprintf(fa, "AA\n");
  fprintf(fb, "AB\n");
  fclose(fb);
  fclose(fa);

  assert(lilv_copy_file("does/not/exist", "copy"));
  assert(lilv_copy_file(a_path, "not/a/dir/copy"));
  assert(!lilv_copy_file(a_path, "copy_c"));
  assert(!lilv_file_equals(a_path, b_path));
  assert(lilv_file_equals(a_path, a_path));
  assert(lilv_file_equals(a_path, "copy_c"));
  assert(!lilv_file_equals("does/not/exist", b_path));
  assert(!lilv_file_equals(a_path, "does/not/exist"));
  assert(!lilv_file_equals("does/not/exist", "/does/not/either"));

  assert(!lilv_remove(a_path));
  assert(!lilv_remove(b_path));
  assert(!lilv_remove(dir));

  lilv_free(b_path);
  lilv_free(a_path);
  lilv_free(dir);
  return 0;
}
