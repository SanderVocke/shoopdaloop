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

  if get_option('strict')
    if cc.get_id() == 'clang'
      c_suppressions += [
        '-Wno-cast-align',
        '-Wno-cast-qual',
        '-Wno-conversion',
        '-Wno-declaration-after-statement',
        '-Wno-double-promotion',
        '-Wno-format-nonliteral',
        '-Wno-nullability-extension',
        '-Wno-nullable-to-nonnull-conversion',
        '-Wno-padded',
        '-Wno-reserved-id-macro',
        '-Wno-sign-conversion',
        '-Wno-switch-enum',
        '-Wno-unused-macros',
      ]

      if host_machine.system() == 'freebsd'
        c_suppressions += [
          '-Wno-c11-extensions',
        ]
      endif

    elif cc.get_id() == 'gcc'
      c_suppressions += [
        '-Wno-cast-align',
        '-Wno-cast-qual',
        '-Wno-format-nonliteral',
        '-Wno-inline',
        '-Wno-padded',
        '-Wno-sign-conversion',
        '-Wno-strict-overflow',
        '-Wno-suggest-attribute=const',
        '-Wno-suggest-attribute=pure',
        '-Wno-switch-default',
        '-Wno-switch-enum',
        '-Wno-unsuffixed-float-constants',
        '-Wno-unused-const-variable',
        '-Wno-unused-macros',
      ]

      if host_machine.system() == 'windows'
        c_suppressions += [
          '-Wno-float-conversion',
          '-Wno-suggest-attribute=format',
        ]
      endif

    elif cc.get_id() == 'msvc'
      c_suppressions += [
        '/wd4061', # enumerator in switch is not explicitly handled
        '/wd4200', # zero-sized array in struct/union
        '/wd4365', # signed/unsigned mismatch
        '/wd4514', # unreferenced inline function has been removed
        '/wd4706', # assignment within conditional expression
        '/wd4710', # function not inlined
        '/wd4711', # function selected for automatic inline expansion
        '/wd4800', # implicit conversion from int to bool
        '/wd4820', # padding added after construct
        '/wd4996', # POSIX name for this item is deprecated
        '/wd4996', # function or variable may be unsafe
        '/wd5045', # will insert Spectre mitigation for memory load
      ]
    endif
  endif

  if cc.get_id() == 'gcc' and host_machine.system() == 'windows'
    c_suppressions += [
      '-Wno-format',
    ]
  endif

  c_suppressions = cc.get_supported_arguments(c_suppressions)
endif

