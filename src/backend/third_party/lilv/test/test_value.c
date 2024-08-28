// Copyright 2007-2020 David Robillard <d@drobilla.net>
// SPDX-License-Identifier: ISC

#undef NDEBUG

#include "lilv_test_utils.h"

#include "lilv/lilv.h"

#include <assert.h>
#include <float.h>
#include <math.h>
#include <string.h>

static const char* const plugin_ttl = "\
:plug a lv2:Plugin ;\n\
	a lv2:CompressorPlugin ;\n\
	doap:name \"Test plugin\" ;\n\
	lv2:port [\n\
		a lv2:ControlPort ; a lv2:InputPort ;\n\
		lv2:index 0 ;\n\
		lv2:symbol \"foo\" ;\n\
		lv2:name \"Foo\" ;\n\
	] .";

int
main(void)
{
  LilvTestEnv* const env   = lilv_test_env_new();
  LilvWorld* const   world = env->world;

  if (create_bundle(env, "value.lv2", SIMPLE_MANIFEST_TTL, plugin_ttl)) {
    return 1;
  }

  lilv_world_load_specifications(env->world);
  lilv_world_load_bundle(env->world, env->test_bundle_uri);

  LilvNode* uval = lilv_new_uri(world, "http://example.org");
  LilvNode* sval = lilv_new_string(world, "Foo");
  LilvNode* ival = lilv_new_int(world, 42);
  LilvNode* fval = lilv_new_float(world, 1.6180f);

  assert(lilv_node_is_uri(uval));
  assert(lilv_node_is_string(sval));
  assert(lilv_node_is_int(ival));
  assert(lilv_node_is_float(fval));

  assert(!lilv_node_is_literal(NULL));
  assert(!lilv_node_is_literal(uval));
  assert(lilv_node_is_literal(sval));
  assert(lilv_node_is_literal(ival));
  assert(lilv_node_is_literal(fval));
  assert(!lilv_node_get_path(fval, NULL));

  assert(!strcmp(lilv_node_as_uri(uval), "http://example.org"));
  assert(!strcmp(lilv_node_as_string(sval), "Foo"));
  assert(lilv_node_as_int(ival) == 42);
  assert(fabs(lilv_node_as_float(fval) - 1.6180) < FLT_EPSILON);
  assert(isnan(lilv_node_as_float(sval)));

#if defined(__clang__)
#  pragma clang diagnostic push
#  pragma clang diagnostic ignored "-Wdeprecated-declarations"
#elif defined(__GNUC__) && __GNUC__ > 4
#  pragma GCC diagnostic push
#  pragma GCC diagnostic ignored "-Wdeprecated-declarations"
#endif

  assert(!strcmp(lilv_uri_to_path("file:///foo"), "/foo"));

#if defined(__clang__)
#  pragma clang diagnostic pop
#elif defined(__GNUC__) && __GNUC__ > 4
#  pragma GCC diagnostic pop
#endif

  LilvNode* loc_abs  = lilv_new_file_uri(world, NULL, "/foo/bar");
  LilvNode* loc_rel  = lilv_new_file_uri(world, NULL, "foo");
  LilvNode* host_abs = lilv_new_file_uri(world, "host", "/foo/bar");
  LilvNode* host_rel = lilv_new_file_uri(world, "host", "foo");

  assert(!strcmp(lilv_node_as_uri(loc_abs), "file:///foo/bar"));
  assert(!strncmp(lilv_node_as_uri(loc_rel), "file:///", 8));
  assert(!strcmp(lilv_node_as_uri(host_abs), "file://host/foo/bar"));
  assert(!strncmp(lilv_node_as_uri(host_rel), "file://host/", 12));

  lilv_node_free(host_rel);
  lilv_node_free(host_abs);
  lilv_node_free(loc_rel);
  lilv_node_free(loc_abs);

  char* tok = lilv_node_get_turtle_token(uval);
  assert(!strcmp(tok, "<http://example.org>"));
  lilv_free(tok);
  tok = lilv_node_get_turtle_token(sval);
  assert(!strcmp(tok, "Foo"));
  lilv_free(tok);
  tok = lilv_node_get_turtle_token(ival);
  assert(!strcmp(tok, "42"));
  lilv_free(tok);
  tok = lilv_node_get_turtle_token(fval);
  assert(!strncmp(tok, "1.6180", 6));
  lilv_free(tok);

  LilvNode* uval_e  = lilv_new_uri(world, "http://example.org");
  LilvNode* sval_e  = lilv_new_string(world, "Foo");
  LilvNode* ival_e  = lilv_new_int(world, 42);
  LilvNode* fval_e  = lilv_new_float(world, 1.6180f);
  LilvNode* uval_ne = lilv_new_uri(world, "http://no-example.org");
  LilvNode* sval_ne = lilv_new_string(world, "Bar");
  LilvNode* ival_ne = lilv_new_int(world, 24);
  LilvNode* fval_ne = lilv_new_float(world, 3.14159f);

  assert(lilv_node_equals(uval, uval_e));
  assert(lilv_node_equals(sval, sval_e));
  assert(lilv_node_equals(ival, ival_e));
  assert(lilv_node_equals(fval, fval_e));

  assert(!lilv_node_equals(uval, uval_ne));
  assert(!lilv_node_equals(sval, sval_ne));
  assert(!lilv_node_equals(ival, ival_ne));
  assert(!lilv_node_equals(fval, fval_ne));

  assert(!lilv_node_equals(uval, sval));
  assert(!lilv_node_equals(sval, ival));
  assert(!lilv_node_equals(ival, fval));

  LilvNode* uval_dup = lilv_node_duplicate(uval);
  assert(lilv_node_equals(uval, uval_dup));

  LilvNode* ifval = lilv_new_float(world, 42.0);
  assert(!lilv_node_equals(ival, ifval));
  lilv_node_free(ifval);

  LilvNode* nil = NULL;
  assert(!lilv_node_equals(uval, nil));
  assert(!lilv_node_equals(nil, uval));
  assert(lilv_node_equals(nil, nil));

  LilvNode* nil2 = lilv_node_duplicate(nil);
  assert(lilv_node_equals(nil, nil2));

  lilv_node_free(uval);
  lilv_node_free(sval);
  lilv_node_free(ival);
  lilv_node_free(fval);
  lilv_node_free(uval_e);
  lilv_node_free(sval_e);
  lilv_node_free(ival_e);
  lilv_node_free(fval_e);
  lilv_node_free(uval_ne);
  lilv_node_free(sval_ne);
  lilv_node_free(ival_ne);
  lilv_node_free(fval_ne);
  lilv_node_free(uval_dup);
  lilv_node_free(nil2);

  delete_bundle(env);
  lilv_test_env_free(env);

  return 0;
}
