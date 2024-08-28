# Copyright 2020-2022 David Robillard <d@drobilla.net>
# SPDX-License-Identifier: 0BSD OR ISC

# Project-specific warning suppressions.
#
# This should be used in conjunction with the generic "warnings" sibling that
# enables all reasonable warnings for the compiler.  It lives here just to keep
# the top-level meson.build more readable.

#####
# C #
#####

if is_variable('cc')
  c_suppressions = []

  if cc.get_id() == 'clang'
    c_suppressions += [
      '-Wno-cast-align',
      '-Wno-cast-qual',
      '-Wno-declaration-after-statement',
      '-Wno-documentation-unknown-command',
      '-Wno-double-promotion',
      '-Wno-float-conversion',
      '-Wno-implicit-float-conversion',
      '-Wno-implicit-int-conversion',
      '-Wno-nullability-extension',
      '-Wno-nullable-to-nonnull-conversion',
      '-Wno-padded',
      '-Wno-shorten-64-to-32',
      '-Wno-sign-conversion',
    ]

  elif cc.get_id() == 'gcc'
    c_suppressions += [
      '-Wno-cast-align',
      '-Wno-cast-qual',
      '-Wno-conversion',
      '-Wno-inline',
      '-Wno-padded',
      '-Wno-suggest-attribute=pure',
      '-Wno-unsuffixed-float-constants',
      '-Wno-unused-const-variable',
    ]

    if host_machine.system() =='windows'
      c_suppressions += [
        '-Wno-suggest-attribute=format',
      ]
    endif

  elif cc.get_id() == 'msvc'
    c_suppressions += [
      '/wd4242', # conversion, possible loss of data
      '/wd4244', # conversion from floating point, possible loss of data
      '/wd4267', # conversion from size_t, possible loss of data
      '/wd4365', # signed/unsigned mismatch
      '/wd4514', # unreferenced inline function has been removed
      '/wd4706', # assignment within conditional expression
      '/wd4710', # function not inlined
      '/wd4711', # function selected for automatic inline expansion
      '/wd4820', # padding added after construct
      '/wd4996', # function or variable may be unsafe
      '/wd5045', # will insert Spectre mitigation for memory load
    ]
  endif

  c_suppressions = cc.get_supported_arguments(c_suppressions)
endif
