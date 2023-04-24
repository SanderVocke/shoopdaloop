# ISC License
#
# Copyright (c) 2023 Sander Vocke
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

# CMake find_package() Module for the Lilv LV2 host library,
#
# If successful the following variables will be defined
#   LILV_FOUND
#   LILV_INCLUDE_DIRS
#
# Example usage:
#   find_package(Lilv)
#   include_directories(${LILV_INCLUDE_DIRS})

if (LILV_INCLUDE_DIRS)
  # in cache already
  set(LILV_FOUND TRUE)
else (LILV_INCLUDE_DIRS)
  find_path(LILV_INCLUDE_DIR
    NAMES
      lilv/lilv.h
    PATHS
      /usr/include/lilv-0
      /usr/local/include/lilv-0
      /opt/local/include/lilv-0
  )
  set(LILV_INCLUDE_DIRS ${LILV_INCLUDE_DIR})

  if (LILV_INCLUDE_DIRS)
    set(LILV_FOUND TRUE)
  endif (LILV_INCLUDE_DIRS)

  mark_as_advanced(LILV_INCLUDE_DIRS)

  if (LILV_FOUND)
    if (NOT LILV_FIND_QUIETLY)
      message(STATUS "Found Lilv: ${LILV_INCLUDE_DIR}")
    endif (NOT LILV_FIND_QUIETLY)
  else (LILV_FOUND)
    if (LILV_FIND_REQUIRED)
      message(FATAL_ERROR "Could not find Lilv")
    endif (LILV_FIND_REQUIRED)
  endif (LILV_FOUND)
endif (LILV_INCLUDE_DIRS)
