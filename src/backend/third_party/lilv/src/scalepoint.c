// Copyright 2007-2019 David Robillard <d@drobilla.net>
// SPDX-License-Identifier: ISC

#include "lilv_internal.h"

#include "lilv/lilv.h"

#include <stdlib.h>

/** Ownership of value and label is taken */
LilvScalePoint*
lilv_scale_point_new(LilvNode* value, LilvNode* label)
{
  LilvScalePoint* point = (LilvScalePoint*)malloc(sizeof(LilvScalePoint));
  point->value          = value;
  point->label          = label;
  return point;
}

void
lilv_scale_point_free(LilvScalePoint* point)
{
  if (point) {
    lilv_node_free(point->value);
    lilv_node_free(point->label);
    free(point);
  }
}

const LilvNode*
lilv_scale_point_get_value(const LilvScalePoint* point)
{
  return point->value;
}

const LilvNode*
lilv_scale_point_get_label(const LilvScalePoint* point)
{
  return point->label;
}
