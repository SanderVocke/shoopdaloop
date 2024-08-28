// Copyright 2015-2020 David Robillard <d@drobilla.net>
// SPDX-License-Identifier: ISC

#undef NDEBUG

#include "../src/filesystem.h"

#include "lilv/lilv.h"
#include "serd/serd.h"

#include <assert.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#define PLUGIN_URI "http://example.org/missing-port"

int
main(int argc, char** argv)
{
  if (argc != 2) {
    fprintf(stderr, "USAGE: %s BUNDLE\n", argv[0]);
    return 1;
  }

  const char* bundle_path = argv[1];
  LilvWorld*  world       = lilv_world_new();

  // Load test plugin bundle
  uint8_t*  abs_bundle = (uint8_t*)lilv_path_absolute(bundle_path);
  SerdNode  bundle     = serd_node_new_file_uri(abs_bundle, 0, 0, true);
  LilvNode* bundle_uri = lilv_new_uri(world, (const char*)bundle.buf);
  lilv_world_load_bundle(world, bundle_uri);
  free(abs_bundle);
  serd_node_free(&bundle);
  lilv_node_free(bundle_uri);

  LilvNode*          plugin_uri = lilv_new_uri(world, PLUGIN_URI);
  const LilvPlugins* plugins    = lilv_world_get_all_plugins(world);
  const LilvPlugin*  plugin     = lilv_plugins_get_by_uri(plugins, plugin_uri);

  // Check that all ports are ignored
  assert(lilv_plugin_get_num_ports(plugin) == 0);

  lilv_node_free(plugin_uri);
  lilv_world_free(world);

  return 0;
}
