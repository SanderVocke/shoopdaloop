# Copyright 2020-2022 David Robillard <d@drobilla.net>
# SPDX-License-Identifier: 0BSD OR ISC

####################
# Project Metadata #
####################

if get_option('strict')
  # Check release metadata
  if not meson.is_subproject()
    autoship = find_program('autoship', required: false)
    if autoship.found()
      test('autoship', autoship, args: ['test', lilv_src_root], suite: 'data')
    endif
  endif

  # Check licensing metadata
  reuse = find_program('reuse', required: false)
  if reuse.found()
    test('REUSE', reuse,
         args: ['--root', lilv_src_root, 'lint'],
         suite: 'data')
  endif
endif

##############
# Test Setup #
##############

test_args = ['-DLILV_STATIC']
if cc.get_id() == 'msvc'
  test_args += [
    '/wd4464', # relative include path contains '..'
  ]
endif

subdir('lv2/core.lv2')

################
# Bundle Tests #
################

subdir('bad_syntax.lv2')
subdir('failed_instantiation.lv2')
subdir('failed_lib_descriptor.lv2')
subdir('lib_descriptor.lv2')
subdir('missing_descriptor.lv2')
subdir('missing_name.lv2')
subdir('missing_plugin.lv2')
subdir('missing_port.lv2')
subdir('missing_port_name.lv2')
subdir('new_version.lv2')
subdir('old_version.lv2')
subdir('test_plugin.lv2')

##############
# Unit Tests #
##############

unit_tests = [
  'bad_port_index',
  'bad_port_symbol',
  'classes',
  'discovery',
  'filesystem',
  'get_symbol',
  'no_author',
  'no_verify',
  'plugin',
  'port',
  'preset',
  'project',
  'project_no_author',
  'prototype',
  'reload_bundle',
  'replace_version',
  'state',
  'string',
  'ui',
  'util',
  'value',
  'verify',
  'world',
]

define_args = [
  '-DLILV_TEST_BUNDLE="@0@/"'.format(meson.current_build_dir() / 'test_plugin.lv2'),
  '-DLILV_TEST_DIR="@0@/"'.format(meson.current_build_dir()),
]

foreach unit : unit_tests
  test(unit,
       executable('test_@0@'.format(unit),
                  files('lilv_test_utils.c', 'test_@0@.c'.format(unit)),
                  c_args: define_args + test_args + c_suppressions,
                  include_directories: include_directories('../src'),
                  dependencies: [lv2_dep, lilv_static_dep]),
       suite: 'unit')
endforeach
