// Copyright 2021 David Robillard <d@drobilla.net>
// SPDX-License-Identifier: ISC

/*
  Configuration header that defines reasonable defaults at compile time.

  This allows compile-time configuration from the command line (typically via
  the build system) while still allowing the source to be built without any
  configuration.  The build system can define LILV_NO_DEFAULT_CONFIG to disable
  defaults, in which case it must define things like HAVE_FEATURE to enable
  features.  The design here ensures that compiler warnings or
  include-what-you-use will catch any mistakes.
*/

#ifndef LILV_CONFIG_H
#define LILV_CONFIG_H

// Define version unconditionally so a warning will catch a mismatch
#define LILV_VERSION "0.24.20"

#if !defined(LILV_NO_DEFAULT_CONFIG)

// We need unistd.h to check _POSIX_VERSION
#  ifndef LILV_NO_POSIX
#    ifdef __has_include
#      if __has_include(<unistd.h>)
#        include <unistd.h>
#      endif
#    elif defined(__unix__)
#      include <unistd.h>
#    endif
#  endif

// POSIX.1-2001: fileno()
#  ifndef HAVE_FILENO
#    if defined(_POSIX_VERSION) && _POSIX_VERSION >= 200112L
#      define HAVE_FILENO
#    endif
#  endif

// Classic UNIX: flock()
#  ifndef HAVE_FLOCK
#    if defined(__unix__) || defined(__APPLE__)
#      define HAVE_FLOCK
#    endif
#  endif

// POSIX.1-2001: lstat()
#  ifndef HAVE_LSTAT
#    if defined(_POSIX_VERSION) && _POSIX_VERSION >= 200112L
#      define HAVE_LSTAT
#    endif
#  endif

#endif // !defined(LILV_NO_DEFAULT_CONFIG)

/*
  Make corresponding USE_FEATURE defines based on the HAVE_FEATURE defines from
  above or the command line.  The code checks for these using #if (not #ifdef),
  so there will be an undefined warning if it checks for an unknown feature,
  and this header is always required by any code that checks for features, even
  if the build system defines them all.
*/

#ifdef HAVE_FILENO
#  define USE_FILENO 1
#else
#  define USE_FILENO 0
#endif

#ifdef HAVE_FLOCK
#  define USE_FLOCK 1
#else
#  define USE_FLOCK 0
#endif

#ifdef HAVE_LSTAT
#  define USE_LSTAT 1
#else
#  define USE_LSTAT 0
#endif

/*
  Define required values.  These are always used as a fallback, even with
  LILV_NO_DEFAULT_CONFIG, since they must be defined for the build to work.
*/

// Separator between entries in variables like PATH
#ifndef LILV_PATH_SEP
#  ifdef _WIN32
#    define LILV_PATH_SEP ";"
#  else
#    define LILV_PATH_SEP ":"
#  endif
#endif

// Separator between directories in a path
#ifndef LILV_DIR_SEP
#  ifdef _WIN32
#    define LILV_DIR_SEP "\\"
#  else
#    define LILV_DIR_SEP "/"
#  endif
#endif

// Default value for LV2_PATH environment variable
#ifndef LILV_DEFAULT_LV2_PATH
#  ifdef _WIN32
#    define LILV_DEFAULT_LV2_PATH "%APPDATA%\\LV2;%COMMONPROGRAMFILES%\\LV2"
#  else
#    define LILV_DEFAULT_LV2_PATH "~/.lv2:/usr/local/lib/lv2:/usr/lib/lv2"
#  endif
#endif

#endif // LILV_CONFIG_H
