// Copyright 2007-2020 David Robillard <d@drobilla.net>
// SPDX-License-Identifier: ISC

#undef NDEBUG

#include "lilv_test_utils.h"

#include "lilv/lilv.h"

#include <assert.h>
#include <stddef.h>

int
main(void)
{
  LilvTestEnv* const env   = lilv_test_env_new();
  LilvWorld* const   world = env->world;

  LilvNode* num = lilv_new_int(env->world, 4);
  LilvNode* uri = lilv_new_uri(env->world, "http://example.org/object");

  LilvNodes* matches = lilv_world_find_nodes(world, num, NULL, NULL);
  assert(!matches);

  matches = lilv_world_find_nodes(world, NULL, num, NULL);
  assert(!matches);

  matches = lilv_world_find_nodes(world, NULL, uri, NULL);
  assert(!matches);

  lilv_node_free(uri);
  lilv_node_free(num);

  lilv_world_unload_bundle(world, NULL);
  lilv_test_env_free(env);

  return 0;
}
