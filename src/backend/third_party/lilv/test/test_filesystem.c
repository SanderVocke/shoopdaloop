// Copyright 2020 David Robillard <d@drobilla.net>
// SPDX-License-Identifier: ISC

#undef NDEBUG

#include "lilv_internal.h"

#include "../src/filesystem.h"

#include <assert.h>
#include <errno.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static bool
equals(char* string, const char* expected)
{
  const bool result = !strcmp(string, expected);
  free(string);
  return result;
}

static void
test_temp_directory_path(void)
{
  char* tmpdir = lilv_temp_directory_path();

  assert(lilv_is_directory(tmpdir));

  free(tmpdir);
}

static void
test_path_is_absolute(void)
{
  assert(lilv_path_is_absolute("/a/b"));
  assert(lilv_path_is_absolute("/a"));
  assert(lilv_path_is_absolute("/"));

  assert(!lilv_path_is_absolute("a/b"));
  assert(!lilv_path_is_absolute("a"));
  assert(!lilv_path_is_absolute("."));

#ifdef _WIN32
  assert(lilv_path_is_absolute("C:/a/b"));
  assert(lilv_path_is_absolute("C:\\a\\b"));
  assert(lilv_path_is_absolute("D:/a/b"));
  assert(lilv_path_is_absolute("D:\\a\\b"));
#endif
}

static void
test_path_is_child(void)
{
  assert(lilv_path_is_child("/a/b", "/a"));
  assert(lilv_path_is_child("/a/b", "/a/"));
  assert(lilv_path_is_child("/a/b/", "/a"));
  assert(lilv_path_is_child("/a/b/", "/a/"));

  assert(!lilv_path_is_child("/a/b", "/a/c"));
  assert(!lilv_path_is_child("/a/b", "/a/c/"));
  assert(!lilv_path_is_child("/a/b/", "/a/c"));
  assert(!lilv_path_is_child("/a/b/", "/a/c/"));

  assert(!lilv_path_is_child("/a/b", "/c"));
  assert(!lilv_path_is_child("/a/b", "/c/"));
  assert(!lilv_path_is_child("/a/b/", "/c"));
  assert(!lilv_path_is_child("/a/b/", "/c/"));
}

static void
test_path_current(void)
{
  char* cwd = lilv_path_current();

  assert(lilv_is_directory(cwd));

  free(cwd);
}

static void
test_path_absolute(void)
{
  const char* const short_path = "a";
  const char* const long_path  = "a/b/c";

  char* const cwd            = lilv_path_current();
  char* const expected_short = lilv_path_join(cwd, short_path);
  char* const expected_long  = lilv_path_join(cwd, long_path);

  assert(equals(lilv_path_absolute(short_path), expected_short));
  assert(equals(lilv_path_absolute(long_path), expected_long));

  free(expected_long);
  free(expected_short);
  free(cwd);
}

static void
test_path_absolute_child(void)
{
  const char* const parent     = "/parent";
  const char* const short_path = "a";
  const char* const long_path  = "a/b/c";

  char* const expected_short = lilv_path_join(parent, short_path);
  char* const expected_long  = lilv_path_join(parent, long_path);

  assert(equals(lilv_path_absolute_child(short_path, parent), expected_short));

  assert(equals(lilv_path_absolute_child(long_path, parent), expected_long));

  free(expected_long);
  free(expected_short);
}

static void
test_path_relative_to(void)
{
  assert(equals(lilv_path_relative_to("/a/b", "/a/"), "b"));
  assert(equals(lilv_path_relative_to("/a", "/b/c/"), "/a"));
  assert(equals(lilv_path_relative_to("/a/b/c", "/a/b/d/"), "../c"));
  assert(equals(lilv_path_relative_to("/a/b/c", "/a/b/d/e/"), "../../c"));

#ifdef _WIN32
  assert(equals(lilv_path_relative_to("C:/a/b", "C:/a/"), "b"));
  assert(equals(lilv_path_relative_to("C:/a", "C:/b/c/"), "../../a"));
  assert(equals(lilv_path_relative_to("C:/a/b/c", "C:/a/b/d/"), "../c"));
  assert(equals(lilv_path_relative_to("C:/a/b/c", "C:/a/b/d/e/"), "../../c"));

  assert(equals(lilv_path_relative_to("C:\\a\\b", "C:\\a\\"), "b"));
  assert(equals(lilv_path_relative_to("C:\\a", "C:\\b\\c\\"), "..\\..\\a"));
  assert(
    equals(lilv_path_relative_to("C:\\a\\b\\c", "C:\\a\\b\\d\\"), "..\\c"));
  assert(equals(lilv_path_relative_to("C:\\a\\b\\c", "C:\\a\\b\\d\\e\\"),
                "..\\..\\c"));
#endif
}

static void
test_path_parent(void)
{
  assert(equals(lilv_path_parent("/"), "/"));
  assert(equals(lilv_path_parent("//"), "/"));
  assert(equals(lilv_path_parent("/a"), "/"));
  assert(equals(lilv_path_parent("/a/"), "/"));
  assert(equals(lilv_path_parent("/a///b/"), "/a"));
  assert(equals(lilv_path_parent("/a///b//"), "/a"));
  assert(equals(lilv_path_parent("/a/b"), "/a"));
  assert(equals(lilv_path_parent("/a/b/"), "/a"));
  assert(equals(lilv_path_parent("/a/b/c"), "/a/b"));
  assert(equals(lilv_path_parent("/a/b/c/"), "/a/b"));
  assert(equals(lilv_path_parent("a"), "."));
}

static void
test_path_filename(void)
{
  // Cases from cppreference.com for std::filesystem::path::filename
  assert(equals(lilv_path_filename("/foo/bar.txt"), "bar.txt"));
  assert(equals(lilv_path_filename("/foo/.bar"), ".bar"));
  assert(equals(lilv_path_filename("/foo/bar/"), ""));
  assert(equals(lilv_path_filename("/foo/."), "."));
  assert(equals(lilv_path_filename("/foo/.."), ".."));
  assert(equals(lilv_path_filename("."), "."));
  assert(equals(lilv_path_filename(".."), ".."));
  assert(equals(lilv_path_filename("/"), ""));
  assert(equals(lilv_path_filename("//host"), "host"));

#ifdef _WIN32
  assert(equals(lilv_path_filename("C:/foo/bar.txt"), "bar.txt"));
  assert(equals(lilv_path_filename("C:\\foo\\bar.txt"), "bar.txt"));
  assert(equals(lilv_path_filename("foo/bar.txt"), "bar.txt"));
  assert(equals(lilv_path_filename("foo\\bar.txt"), "bar.txt"));
#endif
}

static void
test_path_join(void)
{
  assert(lilv_path_join(NULL, NULL) == NULL);
  assert(lilv_path_join(NULL, "") == NULL);

#ifdef _WIN32
  assert(equals(lilv_path_join("", NULL), "\\"));
  assert(equals(lilv_path_join("", ""), "\\"));
  assert(equals(lilv_path_join("a", ""), "a\\"));
  assert(equals(lilv_path_join("a", NULL), "a\\"));
  assert(equals(lilv_path_join("a", "b"), "a\\b"));
#else
  assert(equals(lilv_path_join("", NULL), "/"));
  assert(equals(lilv_path_join("", ""), "/"));
  assert(equals(lilv_path_join("a", ""), "a/"));
  assert(equals(lilv_path_join("a", NULL), "a/"));
  assert(equals(lilv_path_join("a", "b"), "a/b"));
#endif

  assert(equals(lilv_path_join("/a", ""), "/a/"));
  assert(equals(lilv_path_join("/a/b", ""), "/a/b/"));
  assert(equals(lilv_path_join("/a/", ""), "/a/"));
  assert(equals(lilv_path_join("/a/b/", ""), "/a/b/"));
  assert(equals(lilv_path_join("a/b", ""), "a/b/"));
  assert(equals(lilv_path_join("a/", ""), "a/"));
  assert(equals(lilv_path_join("a/b/", ""), "a/b/"));

  assert(equals(lilv_path_join("/a", NULL), "/a/"));
  assert(equals(lilv_path_join("/a/b", NULL), "/a/b/"));
  assert(equals(lilv_path_join("/a/", NULL), "/a/"));
  assert(equals(lilv_path_join("/a/b/", NULL), "/a/b/"));
  assert(equals(lilv_path_join("a/b", NULL), "a/b/"));
  assert(equals(lilv_path_join("a/", NULL), "a/"));
  assert(equals(lilv_path_join("a/b/", NULL), "a/b/"));

  assert(equals(lilv_path_join("/a", "b"), "/a/b"));
  assert(equals(lilv_path_join("/a/", "b"), "/a/b"));
  assert(equals(lilv_path_join("a/", "b"), "a/b"));

  assert(equals(lilv_path_join("/a", "b/"), "/a/b/"));
  assert(equals(lilv_path_join("/a/", "b/"), "/a/b/"));
  assert(equals(lilv_path_join("a", "b/"), "a/b/"));
  assert(equals(lilv_path_join("a/", "b/"), "a/b/"));

#ifdef _WIN32
  assert(equals(lilv_path_join("C:/a", "b"), "C:/a/b"));
  assert(equals(lilv_path_join("C:\\a", "b"), "C:\\a\\b"));
  assert(equals(lilv_path_join("C:/a", "b/"), "C:/a/b/"));
  assert(equals(lilv_path_join("C:\\a", "b\\"), "C:\\a\\b\\"));
#endif
}

static void
test_path_canonical(void)
{
  char* const temp_dir  = lilv_create_temporary_directory("lilvXXXXXX");
  char* const file_path = lilv_path_join(temp_dir, "lilv_test_file");

  FILE* f = fopen(file_path, "w");
  fprintf(f, "test\n");
  fclose(f);

#ifndef _WIN32
  // Test symlink resolution

  char* const link_path = lilv_path_join(temp_dir, "lilv_test_link");

  assert(!lilv_symlink(file_path, link_path));

  char* const real_file_path = lilv_path_canonical(file_path);
  char* const real_link_path = lilv_path_canonical(link_path);

  assert(!strcmp(real_file_path, real_link_path));

  assert(!lilv_remove(link_path));
  free(real_link_path);
  free(real_file_path);
  free(link_path);
#endif

  // Test dot segment resolution

  char* const parent_dir_1      = lilv_path_join(temp_dir, "..");
  char* const parent_dir_2      = lilv_path_parent(temp_dir);
  char* const real_parent_dir_1 = lilv_path_canonical(parent_dir_1);
  char* const real_parent_dir_2 = lilv_path_canonical(parent_dir_2);

  assert(!strcmp(real_parent_dir_1, real_parent_dir_2));

  // Clean everything up

  assert(!lilv_remove(file_path));
  assert(!lilv_remove(temp_dir));

  free(real_parent_dir_2);
  free(real_parent_dir_1);
  free(parent_dir_2);
  free(parent_dir_1);
  free(file_path);
  free(temp_dir);
}

static void
test_path_exists(void)
{
  char* const temp_dir  = lilv_create_temporary_directory("lilvXXXXXX");
  char* const file_path = lilv_path_join(temp_dir, "lilv_test_file");

  assert(!lilv_path_exists(file_path));

  FILE* f = fopen(file_path, "w");
  fprintf(f, "test\n");
  fclose(f);

  assert(lilv_path_exists(file_path));

  assert(!lilv_remove(file_path));
  assert(!lilv_remove(temp_dir));

  free(file_path);
  free(temp_dir);
}

static void
test_is_directory(void)
{
  char* const temp_dir  = lilv_create_temporary_directory("lilvXXXXXX");
  char* const file_path = lilv_path_join(temp_dir, "lilv_test_file");

  assert(lilv_is_directory(temp_dir));
  assert(!lilv_is_directory(file_path)); // Nonexistent

  FILE* f = fopen(file_path, "w");
  fprintf(f, "test\n");
  fclose(f);

  assert(!lilv_is_directory(file_path)); // File

  assert(!lilv_remove(file_path));
  assert(!lilv_remove(temp_dir));

  free(file_path);
  free(temp_dir);
}

static void
test_copy_file(void)
{
  char* const temp_dir  = lilv_create_temporary_directory("lilvXXXXXX");
  char* const file_path = lilv_path_join(temp_dir, "lilv_test_file");
  char* const copy_path = lilv_path_join(temp_dir, "lilv_test_copy");

  FILE* f = fopen(file_path, "w");
  fprintf(f, "test\n");
  fclose(f);

  assert(!lilv_copy_file(file_path, copy_path));
  assert(lilv_file_equals(file_path, copy_path));

  if (lilv_path_exists("/dev/full")) {
    // Copy short file (error after flushing)
    assert(lilv_copy_file(file_path, "/dev/full") == ENOSPC);

    // Copy long file (error during writing)
    f = fopen(file_path, "w");
    for (size_t i = 0; i < 4096; ++i) {
      fprintf(f, "test\n");
    }
    fclose(f);
    assert(lilv_copy_file(file_path, "/dev/full") == ENOSPC);
  }

  assert(!lilv_remove(copy_path));
  assert(!lilv_remove(file_path));
  assert(!lilv_remove(temp_dir));

  free(copy_path);
  free(file_path);
  free(temp_dir);
}

static void
test_flock(void)
{
  char* const temp_dir  = lilv_create_temporary_directory("lilvXXXXXX");
  char* const file_path = lilv_path_join(temp_dir, "lilv_test_file");

  FILE* const f1 = fopen(file_path, "w");
  FILE* const f2 = fopen(file_path, "w");

  assert(!lilv_flock(f1, true, false));
  assert(lilv_flock(f2, true, false));
  assert(!lilv_flock(f1, false, false));

  fclose(f2);
  fclose(f1);
  assert(!lilv_remove(file_path));
  assert(!lilv_remove(temp_dir));
  free(file_path);
  free(temp_dir);
}

typedef struct {
  size_t n_names;
  char** names;
} FileList;

static void
visit(const char* const path, const char* const name, void* const data)
{
  (void)path;

  FileList* file_list = (FileList*)data;

  file_list->names =
    (char**)realloc(file_list->names, sizeof(char*) * ++file_list->n_names);

  file_list->names[file_list->n_names - 1] = lilv_strdup(name);
}

static void
test_dir_for_each(void)
{
  char* const temp_dir = lilv_create_temporary_directory("lilvXXXXXX");
  char* const path1    = lilv_path_join(temp_dir, "lilv_test_1");
  char* const path2    = lilv_path_join(temp_dir, "lilv_test_2");

  FILE* const f1 = fopen(path1, "w");
  FILE* const f2 = fopen(path2, "w");
  fprintf(f1, "test\n");
  fprintf(f2, "test\n");
  fclose(f2);
  fclose(f1);

  FileList file_list = {0, NULL};
  lilv_dir_for_each(temp_dir, &file_list, visit);

  assert((!strcmp(file_list.names[0], "lilv_test_1") &&
          !strcmp(file_list.names[1], "lilv_test_2")) ||
         (!strcmp(file_list.names[0], "lilv_test_2") &&
          !strcmp(file_list.names[1], "lilv_test_1")));

  assert(!lilv_remove(path2));
  assert(!lilv_remove(path1));
  assert(!lilv_remove(temp_dir));

  free(file_list.names[0]);
  free(file_list.names[1]);
  free(file_list.names);
  free(path2);
  free(path1);
  free(temp_dir);
}

static void
test_create_temporary_directory(void)
{
  char* const path1 = lilv_create_temporary_directory("lilvXXXXXX");

  assert(lilv_is_directory(path1));

  char* const path2 = lilv_create_temporary_directory("lilvXXXXXX");

  assert(strcmp(path1, path2));
  assert(lilv_is_directory(path2));

  assert(!lilv_remove(path2));
  assert(!lilv_remove(path1));
  free(path2);
  free(path1);
}

static void
test_create_directories(void)
{
  char* const temp_dir = lilv_create_temporary_directory("lilvXXXXXX");

  assert(lilv_is_directory(temp_dir));

  char* const child_dir      = lilv_path_join(temp_dir, "child");
  char* const grandchild_dir = lilv_path_join(child_dir, "grandchild");

  assert(!lilv_create_directories(grandchild_dir));
  assert(lilv_is_directory(grandchild_dir));
  assert(lilv_is_directory(child_dir));

  char* const file_path = lilv_path_join(temp_dir, "lilv_test_file");
  FILE* const f         = fopen(file_path, "w");

  fprintf(f, "test\n");
  fclose(f);

  assert(lilv_create_directories(file_path));

  assert(!lilv_remove(file_path));
  assert(!lilv_remove(grandchild_dir));
  assert(!lilv_remove(child_dir));
  assert(!lilv_remove(temp_dir));
  free(file_path);
  free(child_dir);
  free(grandchild_dir);
  free(temp_dir);
}

static void
test_file_equals(void)
{
  char* const temp_dir = lilv_create_temporary_directory("lilvXXXXXX");
  char* const path1    = lilv_path_join(temp_dir, "lilv_test_1");
  char* const path2    = lilv_path_join(temp_dir, "lilv_test_2");

  FILE* const f1 = fopen(path1, "w");
  FILE* const f2 = fopen(path2, "w");
  fprintf(f1, "test\n");
  fprintf(f2, "test\n");

  assert(lilv_file_equals(path1, path2));

  fprintf(f2, "diff\n");
  fflush(f2);

  assert(!lilv_file_equals(path1, path2));

  fclose(f2);
  fclose(f1);

  assert(!lilv_remove(path2));
  assert(!lilv_remove(path1));
  assert(!lilv_remove(temp_dir));

  free(path2);
  free(path1);
  free(temp_dir);
}

int
main(void)
{
  test_temp_directory_path();
  test_path_is_absolute();
  test_path_is_child();
  test_path_current();
  test_path_absolute();
  test_path_absolute_child();
  test_path_relative_to();
  test_path_parent();
  test_path_filename();
  test_path_join();
  test_path_canonical();
  test_path_exists();
  test_is_directory();
  test_copy_file();
  test_flock();
  test_dir_for_each();
  test_create_temporary_directory();
  test_create_directories();
  test_file_equals();

  return 0;
}
