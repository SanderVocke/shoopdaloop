# Copyright 2021-2022 David Robillard <d@drobilla.net>
# SPDX-License-Identifier: 0BSD OR ISC

project('lilv', ['c'],
        version: '0.24.20',
        license: 'ISC',
        meson_version: '>= 0.56.0',
        default_options: [
          'b_ndebug=if-release',
          'buildtype=release',
          'c_std=c99',
        ])

lilv_src_root = meson.current_source_dir()
major_version = meson.project_version().split('.')[0]
version_suffix = '-@0@'.format(major_version)
versioned_name = 'lilv' + version_suffix

#######################
# Compilers and Flags #
#######################

# Load build tools
pkg = import('pkgconfig')
cc = meson.get_compiler('c')

# Set global warning flags
if get_option('strict') and not meson.is_subproject()
  subdir('meson/warnings')
endif
subdir('meson/suppressions')

##########################
# Platform Configuration #
##########################

platform_defines = ['-DLILV_VERSION="@0@"'.format(meson.project_version())]
if host_machine.system() == 'darwin'
  platform_defines += [
    '-D_DARWIN_C_SOURCE',
  ]
elif host_machine.system() in ['gnu', 'linux']
  platform_defines += [
    '-D_DEFAULT_SOURCE',
    '-D_POSIX_C_SOURCE=200809L',
  ]
endif

default_lv2_path = get_option('default_lv2_path')
if default_lv2_path == ''
  if host_machine.system() == 'darwin'
    lv2_dirs = [
      '~/.lv2',
      '~/Library/Audio/Plug-Ins/LV2',
      '/usr/local/lib/lv2',
      '/usr/lib/lv2',
      '/Library/Audio/Plug-Ins/LV2',
    ]

    default_lv2_path = ':'.join(lv2_dirs)

  elif host_machine.system() == 'haiku'
    default_lv2_path = ':'.join(['~/.lv2', '/boot/common/add-ons/lv2'])

  elif host_machine.system() == 'windows'
    lv2_dirs = ['%%APPDATA%%\\\\LV2', '%%COMMONPROGRAMFILES%%\\\\LV2']
    default_lv2_path = ';'.join(lv2_dirs)

  else
    lv2_dirs = [
      '~' / '.lv2',
      '/usr/local' / get_option('libdir') / 'lv2',
      '/usr' / get_option('libdir') / 'lv2',
    ]

    default_lv2_path = ':'.join(lv2_dirs)
  endif
endif

platform_defines += [
  '-DLILV_DEFAULT_LV2_PATH="@0@"'.format(default_lv2_path)
]

add_project_arguments(platform_defines, language: ['c'])

################
# Dependencies #
################

m_dep = cc.find_library('m', required: false)
dl_dep = cc.find_library('dl', required: false)

lv2_dep = dependency('lv2',
                     version: '>= 1.18.2',
                     fallback: ['lv2', 'lv2_dep'])

serd_dep = dependency('serd-0',
                      version: '>= 0.30.10',
                      fallback: ['serd', 'serd_dep'])

sord_dep = dependency('sord-0',
                      version: '>= 0.16.10',
                      fallback: ['sord', 'sord_dep'])

sratom_dep = dependency('sratom-0',
                        version: '>= 0.6.10',
                        fallback: ['sratom', 'sratom_dep'])

###########
# Library #
###########

c_headers = files('include/lilv/lilv.h')
cpp_headers = files('include/lilv/lilvmm.hpp')

sources = files(
  'src/collections.c',
  'src/filesystem.c',
  'src/instance.c',
  'src/lib.c',
  'src/node.c',
  'src/plugin.c',
  'src/pluginclass.c',
  'src/port.c',
  'src/query.c',
  'src/scalepoint.c',
  'src/state.c',
  'src/ui.c',
  'src/util.c',
  'src/world.c',
  'src/zix/tree.c',
)

# Set appropriate arguments for building against the library type
extra_c_args = []
subdir('meson/library')
if get_option('default_library') == 'static'
  extra_c_args = ['-DLILV_STATIC']
endif

# Build main shared and/or static library
liblilv = library(
  meson.project_name() + library_suffix,
  sources,
  c_args: c_suppressions +  extra_c_args + ['-DLILV_INTERNAL', '-DZIX_STATIC'],
  dependencies: [m_dep, dl_dep, lv2_dep, serd_dep, sord_dep, sratom_dep],
  gnu_symbol_visibility: 'hidden',
  include_directories: include_directories('include', 'src'),
  install: true,
  version: meson.project_version(),
)

# Declare dependency for internal meson dependants
lilv_dep = declare_dependency(
  compile_args: extra_c_args,
  dependencies: [m_dep, dl_dep, lv2_dep, serd_dep, sord_dep, sratom_dep],
  include_directories: include_directories('include'),
  link_with: liblilv,
)

# Generage pkg-config file for external dependants
pkg.generate(
  liblilv,
  description: 'Library for hosting LV2 plugins',
  extra_cflags: extra_c_args,
  filebase: versioned_name,
  name: 'Lilv',
  subdirs: [versioned_name],
  version: meson.project_version(),
)

# Install headers to a versioned include directory
install_headers(c_headers, subdir: versioned_name / 'lilv')
install_headers(cpp_headers, subdir: versioned_name / 'lilv')

#########
# Tools #
#########

subdir('tools')

############
# Bindings #
############

subdir('bindings/python')

###########
# Support #
###########

if not get_option('docs').disabled()
  subdir('doc')
endif

if not get_option('tests').disabled()
  # Get or build a static library for linking internal tests
  if get_option('default_library') == 'both'
    liblilv_static = liblilv.get_static_lib()
  elif get_option('default_library') == 'shared'
    liblilv_static = static_library(
      meson.project_name() + library_suffix,
      sources,
      include_directories: include_directories('include', 'src'),
      c_args: c_suppressions + ['-DLILV_INTERNAL', '-DLILV_STATIC', '-DZIX_STATIC'],
      dependencies: [m_dep, dl_dep, lv2_dep, serd_dep, sord_dep, sratom_dep],
      gnu_symbol_visibility: 'default')
  else
    liblilv_static = liblilv
  endif

  lilv_static_dep = declare_dependency(
    compile_args: extra_c_args,
    dependencies: [m_dep, dl_dep, lv2_dep, serd_dep, sord_dep, sratom_dep],
    include_directories: include_directories('include'),
    link_with: liblilv_static,
  )

  # Build and run tests against static library
  subdir('test')
endif

if not meson.is_subproject()
  summary('Tests', not get_option('tests').disabled(), bool_yn: true)
  summary('Tools', not get_option('tools').disabled(), bool_yn: true)

  summary('Install prefix', get_option('prefix'))
  summary('Default LV2_PATH', default_lv2_path)

  summary('Headers', get_option('prefix') / get_option('includedir'))
  summary('Libraries', get_option('prefix') / get_option('libdir'))

  if not get_option('tools').disabled()
    summary('Executables', get_option('prefix') / get_option('bindir'))
    summary('Man pages', get_option('prefix') / get_option('mandir'))
  endif
endif
