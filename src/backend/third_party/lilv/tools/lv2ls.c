// Copyright 2007-2019 David Robillard <d@drobilla.net>
// SPDX-License-Identifier: ISC

#include "lilv_config.h"

#include "lilv/lilv.h"

#include <stdbool.h>
#include <stdio.h>
#include <string.h>

static void
list_plugins(const LilvPlugins* list, bool show_names)
{
  LILV_FOREACH (plugins, i, list) {
    const LilvPlugin* p = lilv_plugins_get(list, i);
    if (show_names) {
      LilvNode* n = lilv_plugin_get_name(p);
      printf("%s\n", lilv_node_as_string(n));
      lilv_node_free(n);
    } else {
      printf("%s\n", lilv_node_as_uri(lilv_plugin_get_uri(p)));
    }
  }
}

static void
print_version(void)
{
  printf("lv2ls (lilv) " LILV_VERSION "\n"
         "Copyright 2007-2021 David Robillard <d@drobilla.net>\n"
         "License: <http://www.opensource.org/licenses/isc-license>\n"
         "This is free software: you are free to change and redistribute it.\n"
         "There is NO WARRANTY, to the extent permitted by law.\n");
}

static void
print_usage(void)
{
  printf("Usage: lv2ls [OPTION]...\n");
  printf("List all installed LV2 plugins.\n");
  printf("\n");
  printf("  -n, --names    Show names instead of URIs\n");
  printf("  --help         Display this help and exit\n");
  printf("  --version      Display version information and exit\n");
  printf("\n");
  printf("The environment variable LV2_PATH can be used to control where\n");
  printf(
    "this (and all other lilv based LV2 hosts) will search for plugins.\n");
}

int
main(int argc, char** argv)
{
  bool show_names = false;
  for (int i = 1; i < argc; ++i) {
    if (!strcmp(argv[i], "--names") || !strcmp(argv[i], "-n")) {
      show_names = true;
    } else if (!strcmp(argv[i], "--version")) {
      print_version();
      return 0;
    } else if (!strcmp(argv[i], "--help")) {
      print_usage();
      return 0;
    } else {
      print_usage();
      return 1;
    }
  }

  LilvWorld* world = lilv_world_new();
  lilv_world_load_all(world);

  const LilvPlugins* plugins = lilv_world_get_all_plugins(world);

  list_plugins(plugins, show_names);

  lilv_world_free(world);

  return 0;
}
