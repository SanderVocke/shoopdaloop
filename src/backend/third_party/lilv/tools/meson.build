# Copyright 2021-2022 David Robillard <d@drobilla.net>
# SPDX-License-Identifier: 0BSD OR ISC

include_dirs = include_directories('../src')

#############################
# "Basic" (Lilv-Only) Tools #
#############################

basic_tools = [
  'lv2info',
  'lv2ls',
]

foreach tool : basic_tools
  exe = executable(
    tool,
    files(tool + '.c'),
    c_args: c_suppressions,
    dependencies: lilv_dep,
    include_directories: include_dirs,
    install: true,
  )

  meson.override_find_program(tool, exe)

  install_man(files('..' / 'doc' / tool + '.1'))
endforeach

install_data(
  files('lilv.bash_completion'),
  install_dir: get_option('sysconfdir') / 'bash_completion.d',
  rename: 'lilv',
)

###########################
# lv2apply (uses sndfile) #
###########################

sndfile_dep = dependency(
  'sndfile',
  version: '>= 1.0.0',
  required: get_option('tools'),
)

if sndfile_dep.found()
  lv2apply = executable(
    'lv2apply',
    files('lv2apply.c'),
    c_args: c_suppressions,
    dependencies: [lilv_dep, sndfile_dep],
    include_directories: include_dirs,
    install: true,
  )

  meson.override_find_program('lv2apply', lv2apply)

  install_man(files('..' / 'doc' / 'lv2apply.1'))
endif

#################################
# lv2bench (uses clock_gettime) #
#################################

if host_machine.system() != 'windows'
  rt_dep = cc.find_library('rt', required: false)

  clock_gettime_code = '''#include <time.h>
int main(void) { struct timespec t; return clock_gettime(CLOCK_MONOTONIC, &t); }
'''

  if cc.compiles(clock_gettime_code,
                 args: platform_defines,
                 dependencies: [rt_dep],
                 name: 'clock_gettime')
    lv2bench = executable(
      'lv2bench',
      files('lv2bench.c'),
      c_args: c_suppressions,
      dependencies: [lilv_dep, rt_dep, sndfile_dep],
      include_directories: include_dirs,
      install: true,
    )

    meson.override_find_program('lv2bench', lv2bench)

    install_man(files('..' / 'doc' / 'lv2bench.1'))
  endif
endif
