# ISC License
#
# Copyright (c) 2018 Jakob Dübel
#
# Permission to use, copy, modify, and/or distribute this software for
# any purpose with or without fee is hereby granted, provided that the
# above copyright notice and this permission notice appear in all
# copies.
#
# THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL
# WARRANTIES WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED
# WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE
# AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL
# DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA
# OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER
# TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR
# PERFORMANCE OF THIS SOFTWARE.

# CMake find_package() Module for the LV2 audio plugin standard
# http://lv2plug.in/
#
# If successful the following variables will be defined
#   LV2_FOUND
#   LV2_INCLUDE_DIRS
#
# Example usage:
#   find_package(LV2)
#   include_directories(${LV2_INCLUDE_DIRS})

if (LV2_INCLUDE_DIRS)
  # in cache already
  set(LV2_FOUND TRUE)
else (LV2_INCLUDE_DIRS)
  find_path(LV2_INCLUDE_DIR
    NAMES
      lv2.h
    PATHS
      /usr/include
      /usr/local/include
      /opt/local/include
  )
  set(LV2_INCLUDE_DIRS ${LV2_INCLUDE_DIR})

  if (LV2_INCLUDE_DIRS)
    set(LV2_FOUND TRUE)
  endif (LV2_INCLUDE_DIRS)

  mark_as_advanced(LV2_INCLUDE_DIRS)

  if (LV2_FOUND)
    if (NOT LV2_FIND_QUIETLY)
      message(STATUS "Found LV2: ${LV2_INCLUDE_DIR}")
    endif (NOT LV2_FIND_QUIETLY)
  else (LV2_FOUND)
    if (LV2_FIND_REQUIRED)
      message(FATAL_ERROR "Could not find LV2")
    endif (LV2_FIND_REQUIRED)
  endif (LV2_FOUND)
endif (LV2_INCLUDE_DIRS)
