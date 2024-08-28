# Copyright 2021-2022 David Robillard <d@drobilla.net>
# SPDX-License-Identifier: 0BSD OR ISC

####################
# Project Metadata #
####################

if get_option('strict')
  # Check release metadata
  if not meson.is_subproject()
    autoship = find_program('autoship', required: false)
    if autoship.found()
      test('autoship', autoship, args: ['test', sord_src_root], suite: 'data')
    endif
  endif

  # Check licensing metadata
  reuse = find_program('reuse', required: false)
  if reuse.found()
    test('REUSE', reuse,
         args: ['--root', sord_src_root, 'lint'],
         suite: 'data')
  endif
endif

##############
# Unit Tests #
##############

unit_tests = [
  'sord',
]

foreach unit : unit_tests
  test(
    unit,
    executable(
      'test_@0@'.format(unit),
      files('test_@0@.c'.format(unit)),
      c_args: c_suppressions,
      dependencies: sord_dep,
    ),
    suite: 'unit',
  )
endforeach
